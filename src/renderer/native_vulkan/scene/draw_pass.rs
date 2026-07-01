use std::path::PathBuf;
use std::sync::Arc;

use crate::core::scene::{
    SceneMesh, SceneMeshVertex, SceneNativeEffectMotion, SceneSnapshotLayer,
    SceneSnapshotSampledImageLayer,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneTextAlign,
    SceneTextureRegion, SceneTransform,
};
use crate::renderer::native_vulkan::effect_debug::{
    NativeVulkanEffectDebugR8UvGroup, native_vulkan_effect_debug_enabled,
    native_vulkan_effect_debug_log, native_vulkan_effect_debug_r8_gtex_group_report,
};
use crate::renderer::{SceneRenderAlphaTextureMode, SceneRenderLayer};

mod blend;
mod classification;
mod color;
mod effect;
mod effect_graph;
mod geometry_common;
mod plan;
mod texture_slots;
mod types;

use self::blend::native_vulkan_scene_blend_state;
pub(super) use self::classification::native_vulkan_scene_render_layer_is_clear;
use self::classification::{
    native_vulkan_scene_background_clear_color, native_vulkan_scene_draw_pass_op_buckets,
    native_vulkan_scene_fast_clear_color, native_vulkan_scene_full_extent_sampled_image_op_count,
    native_vulkan_scene_full_extent_sampled_image_op_ready,
};
use self::color::{
    native_vulkan_scene_rectangle_recordable_kind, native_vulkan_scene_rgba_from_hex,
    native_vulkan_scene_tint_from_color,
};
use self::effect::motion as sampled_image_motion;
pub(super) use self::effect::native_vulkan_scene_effect_passes_from_render_passes;
use self::effect::{
    native_vulkan_scene_effect_passes_from_scene_passes, native_vulkan_scene_effect_records_label,
    native_vulkan_scene_material_pass_label,
    native_vulkan_scene_render_first_class_effect_target_pass,
    native_vulkan_scene_sampled_image_material_pass,
    native_vulkan_scene_sampled_image_material_pass_with_effect_blend,
    native_vulkan_scene_snapshot_first_class_effect_target_pass,
};
pub(super) use self::effect_graph::{
    native_vulkan_scene_we_image_graph_plan, native_vulkan_scene_we_image_pass_chain,
};
use self::geometry_common::{
    native_vulkan_scene_quad_positions, native_vulkan_scene_sampled_image_index_buffer_bytes,
    native_vulkan_scene_sampled_image_vertex_buffer_bytes,
    native_vulkan_scene_solid_index_buffer_bytes, native_vulkan_scene_solid_vertex_buffer_bytes,
    native_vulkan_scene_transform_point, native_vulkan_scene_transform_point_with_rotation,
};
pub(super) use self::plan::native_vulkan_scene_draw_pass_plan;
use self::texture_slots::{
    native_vulkan_scene_sampled_image_source_index,
    native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots,
    native_vulkan_scene_texture_slots_from_render_slots,
    native_vulkan_scene_texture_slots_from_scene_slots,
};
pub(super) use self::types::{
    NativeVulkanSceneBlendEquation, NativeVulkanSceneBlendFactor, NativeVulkanSceneBlendOp,
    NativeVulkanSceneBlendState, NativeVulkanSceneCullMode, NativeVulkanSceneDrawPassPlan,
    NativeVulkanSceneEffectEvaluationBoundary, NativeVulkanSceneEffectKind,
    NativeVulkanSceneEffectRecord, NativeVulkanSceneMaterialFlag, NativeVulkanSceneMaterialKind,
    NativeVulkanSceneMaterialPass, NativeVulkanSceneQuadRecordingStep, NativeVulkanSceneQuadVertex,
    NativeVulkanSceneRecordableQuad, NativeVulkanSceneRenderState,
    NativeVulkanSceneSampledImageEffectPass, NativeVulkanSceneSampledImageEffectTarget,
    NativeVulkanSceneSampledImageGeometryRange, NativeVulkanSceneSampledImageQuad,
    NativeVulkanSceneSampledImageRecordingStep, NativeVulkanSceneSampledImageRenderTarget,
    NativeVulkanSceneSampledImageVertex, NativeVulkanSceneTextureSlot,
    NativeVulkanSceneTextureSlotResourceBinding, NativeVulkanSceneVideoQuad,
    NativeVulkanSceneVideoRecordingStep, NativeVulkanSceneWeImageGraphPlan,
    NativeVulkanSceneWeImageGraphStep, NativeVulkanSceneWeImageGraphTarget,
    NativeVulkanSceneWeImageGraphTextureBinding, NativeVulkanSceneWeImageGraphTextureBindingSource,
    NativeVulkanSceneWeImagePass, NativeVulkanSceneWeImagePassChain,
    NativeVulkanSceneWeImagePassEndpoint, NativeVulkanSceneWeImagePassExecution,
    NativeVulkanSceneWeImagePassRole,
};
use super::super::present::render_plan::{
    NativeVulkanSceneDrawOp, NativeVulkanSceneDrawOpKind, NativeVulkanSceneDrawPlan,
    NativeVulkanSceneEffectUvBounds, NativeVulkanSceneEffectUvMapping,
    NativeVulkanSceneEffectUvSpace, native_vulkan_scene_effect_uv_space_from_transform,
    native_vulkan_scene_effect_uv_transform_for_render_passes,
    native_vulkan_scene_effect_uv_transform_for_scene_passes,
};

const SCENE_FULL_SOLID_QUAD_VERTEX_BYTES: u64 = 24;
const SCENE_FULL_SOLID_QUAD_INDEX_BYTES: u64 = 4;
const SCENE_FULL_ELLIPSE_SEGMENTS: usize = 48;
const SCENE_FULL_ROUNDED_RECT_CORNER_SEGMENTS: usize = 8;
const SCENE_FULL_TEXT_DEFAULT_FONT_SIZE: f64 = 24.0;
const SCENE_FULL_TEXT_GLYPH_COLUMNS: usize = 5;
const SCENE_FULL_TEXT_GLYPH_ROWS: usize = 7;
const SCENE_FULL_TEXT_GLYPH_ADVANCE_COLUMNS: f64 = 6.0;
const SCENE_FULL_TEXT_LINE_ADVANCE_ROWS: f64 = 8.0;
const SCENE_FULL_PATH_POINT_EPSILON: f64 = 1.0e-9;
const SCENE_FULL_PATH_CURVE_SEGMENTS: usize = 16;
const SCENE_FULL_PATH_ARC_SEGMENTS_PER_QUARTER: usize = 8;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT: u32 = 4;
const SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT: u32 = 6;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES: u64 = 44;
const SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES: u64 = 4;
const SCENE_SAMPLED_IMAGE_DEFAULT_TINT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneSampledImageGridCorner {
    x: f64,
    y: f64,
    u: f64,
    v: f64,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneSampledImageMeshPoint {
    x: f64,
    y: f64,
    u: f64,
    v: f64,
    opacity: f64,
}

pub(super) fn native_vulkan_scene_solid_geometry_from_render_layer(
    layer_index: usize,
    layer: &SceneRenderLayer,
) -> Result<Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)>, &'static str> {
    if layer.opacity <= 0.0 {
        return Ok(None);
    }
    if native_vulkan_scene_render_layer_is_clear(layer) {
        return Ok(None);
    }
    let kind = match layer.kind {
        SceneNodeKind::Rectangle => {
            if native_vulkan_scene_render_layer_has_shape_paint(layer) {
                native_vulkan_scene_render_layer_rectangle_kind(layer)
            } else {
                return Err("rectangle-layer-missing-paint");
            }
        }
        SceneNodeKind::Ellipse => {
            if native_vulkan_scene_render_layer_has_shape_paint(layer) {
                "ellipse"
            } else {
                return Err("ellipse-layer-missing-paint");
            }
        }
        SceneNodeKind::Text => {
            if layer
                .text
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
                && layer.color.as_ref().is_some_and(|color| !color.is_empty())
            {
                "text"
            } else {
                return Err("text-layer-missing-text-or-color");
            }
        }
        SceneNodeKind::Path => {
            if layer
                .path_data
                .as_ref()
                .is_some_and(|path| !path.is_empty())
                && native_vulkan_scene_render_layer_has_shape_paint(layer)
            {
                "path"
            } else {
                return Err("path-layer-missing-data-or-paint");
            }
        }
        SceneNodeKind::Color => return Err("color-layer-needs-clear-or-rectangle-shape"),
        SceneNodeKind::Image => return Err("image-layer-needs-sampled-image-runtime"),
        SceneNodeKind::Video => return Err("video-layer-needs-vulkan-video-scene-bridge"),
        SceneNodeKind::Audio => return Err("audio-layer-has-no-visual-draw-op"),
        SceneNodeKind::Group => return Err("group-layer-needs-flattened-children"),
        SceneNodeKind::Shader => return Err("shader-layer-needs-scene-shader-runtime"),
        SceneNodeKind::ParticleEmitter => {
            return Err("particle-layer-needs-scene-particle-runtime");
        }
        SceneNodeKind::AudioResponse => {
            if native_vulkan_scene_render_layer_has_shape_paint(layer)
                && layer
                    .width
                    .is_some_and(|width| width.is_finite() && width > 0.0)
                && layer
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
            {
                "audio-response"
            } else {
                return Err("audio-response-layer-missing-native-visual-geometry");
            }
        }
        SceneNodeKind::Script => return Err("script-layer-needs-scene-script-runtime"),
        SceneNodeKind::Unknown => return Err("unknown-layer-kind"),
    };
    let Some(quad) =
        native_vulkan_scene_recordable_quad_from_render_layer(layer_index, layer, kind)
    else {
        return Err("solid-layer-missing-paint");
    };
    if !native_vulkan_scene_solid_has_recordable_geometry(&quad) {
        return Err("solid-layer-missing-recordable-geometry");
    }
    Ok(native_vulkan_scene_solid_geometry(&quad))
}

pub(super) fn native_vulkan_scene_sampled_image_geometry_from_render_layer(
    layer_index: usize,
    layer: &SceneRenderLayer,
) -> Result<
    Option<(
        PathBuf,
        FitMode,
        Option<SceneTextureRegion>,
        Vec<NativeVulkanSceneSampledImageVertex>,
        Vec<u32>,
    )>,
    &'static str,
> {
    let Some((fit, texture_region, vertices, indices)) =
        native_vulkan_scene_sampled_image_geometry_payload_from_render_layer(layer_index, layer)?
    else {
        return Ok(None);
    };
    let Some(source) = layer.source.clone() else {
        return Err("image-layer-missing-source");
    };
    Ok(Some((source, fit, texture_region, vertices, indices)))
}

pub(super) fn native_vulkan_scene_sampled_image_geometry_payload_from_render_layer(
    layer_index: usize,
    layer: &SceneRenderLayer,
) -> Result<
    Option<(
        FitMode,
        Option<SceneTextureRegion>,
        Vec<NativeVulkanSceneSampledImageVertex>,
        Vec<u32>,
    )>,
    &'static str,
> {
    if layer.opacity <= 0.0 {
        return Ok(None);
    }
    if native_vulkan_scene_render_layer_is_clear(layer) {
        return Ok(None);
    }
    if layer.kind != SceneNodeKind::Image {
        return Err("non-image-layer-needs-non-sampled-runtime");
    }
    if layer.source.is_none() {
        return Err("image-layer-missing-source");
    }
    let mesh = layer.mesh.clone();
    let (width, height) = if mesh.is_some() {
        (layer.width.unwrap_or(0.0), layer.height.unwrap_or(0.0))
    } else {
        (
            layer.width.ok_or("image-layer-missing-width")?,
            layer.height.ok_or("image-layer-missing-height")?,
        )
    };
    let texture_slots = native_vulkan_scene_texture_slots_from_render_slots(&layer.texture_slots);
    let effect_passes =
        native_vulkan_scene_effect_passes_from_render_passes(&layer.image_effect_passes);
    let material_pass = native_vulkan_scene_sampled_image_material_pass(
        NativeVulkanSceneMaterialKind::SampledImage,
        layer.blend_mode,
        layer.alpha_texture_slot,
        layer.alpha_texture_mode,
        texture_slots.len(),
        &effect_passes,
    );
    let quad = NativeVulkanSceneSampledImageQuad {
        layer_index,
        layer_id: layer.id.clone(),
        source: PathBuf::new(),
        texture_slots,
        image_effect_pass_count: effect_passes.len(),
        effect_target_pass: native_vulkan_scene_render_first_class_effect_target_pass(
            &layer.image_effect_passes,
        ),
        material_pass,
        base_blend_mode: layer.blend_mode,
        effect_passes,
        composite_key: layer.composite_key.clone(),
        fit: layer.fit,
        opacity: layer.opacity,
        tint: native_vulkan_scene_tint_from_color(layer.color.as_deref()),
        width,
        height,
        mesh,
        effect_uv_space: None,
        effect_motion: layer.effect_motion,
        texture_region: layer.texture_region,
        transform: layer.transform,
    };
    if !native_vulkan_scene_sampled_image_quad_has_recordable_geometry(&quad) {
        return Err("image-layer-missing-recordable-geometry");
    }
    let (vertices, indices) = native_vulkan_scene_sampled_image_geometry(&quad)
        .ok_or("image-layer-missing-recordable-geometry")?;
    Ok(Some((layer.fit, layer.texture_region, vertices, indices)))
}

pub(super) fn native_vulkan_scene_append_sampled_image_geometry_from_render_layer(
    layer_index: usize,
    layer: &SceneRenderLayer,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Result<
    Option<(
        FitMode,
        Option<SceneTextureRegion>,
        NativeVulkanSceneSampledImageGeometryRange,
    )>,
    &'static str,
> {
    native_vulkan_scene_append_sampled_image_geometry_from_layer_parts(
        layer_index,
        &layer.id,
        layer.kind,
        layer.source.is_some(),
        layer.fit,
        layer.opacity,
        layer.width,
        layer.height,
        layer.mesh.clone(),
        layer.effect_motion,
        layer.blend_mode,
        native_vulkan_scene_tint_from_color(layer.color.as_deref()),
        layer.texture_region,
        layer.transform,
        layer.alpha_texture_slot,
        layer.alpha_texture_mode,
        native_vulkan_scene_opacity_effect_uv_space_from_render_layer(layer),
        vertices,
        indices,
    )
}

pub(super) fn native_vulkan_scene_append_sampled_image_geometry_from_snapshot_layer(
    layer_index: usize,
    layer: &SceneSnapshotLayer,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Result<
    Option<(
        FitMode,
        Option<SceneTextureRegion>,
        NativeVulkanSceneSampledImageGeometryRange,
    )>,
    &'static str,
> {
    native_vulkan_scene_append_sampled_image_geometry_from_layer_parts(
        layer_index,
        &layer.id,
        layer.kind,
        layer.source.is_some(),
        layer.fit,
        layer.opacity,
        layer.width,
        layer.height,
        layer.mesh.clone(),
        layer.effect_motion,
        layer.blend_mode,
        native_vulkan_scene_tint_from_color(layer.color.as_deref()),
        layer.texture_region,
        layer.transform,
        layer.alpha_texture_slot,
        layer.alpha_texture_mode.into(),
        native_vulkan_scene_opacity_effect_uv_space_from_snapshot_layer(layer),
        vertices,
        indices,
    )
}

pub(super) fn native_vulkan_scene_append_sampled_image_vertices_from_snapshot_layer(
    layer_index: usize,
    layer: &SceneSnapshotLayer,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Result<Option<u32>, &'static str> {
    native_vulkan_scene_append_sampled_image_vertices_from_layer_parts(
        layer_index,
        layer.kind,
        layer.source.is_some(),
        layer.fit,
        layer.opacity,
        layer.width,
        layer.height,
        layer.mesh.clone(),
        layer.effect_motion,
        layer.blend_mode,
        native_vulkan_scene_tint_from_color(layer.color.as_deref()),
        layer.texture_region,
        layer.transform,
        layer.alpha_texture_slot,
        layer.alpha_texture_mode.into(),
        native_vulkan_scene_opacity_effect_uv_space_from_snapshot_layer(layer),
        vertices,
    )
}

fn native_vulkan_scene_opacity_effect_uv_space_from_render_layer(
    layer: &SceneRenderLayer,
) -> Option<NativeVulkanSceneEffectUvSpace> {
    layer.alpha_texture_slot?;
    Some(native_vulkan_scene_effect_uv_space_from_transform(
        native_vulkan_scene_effect_uv_transform_for_render_passes(
            &layer.image_effect_passes,
            layer.alpha_texture_slot,
        ),
        layer.width.unwrap_or(0.0),
        layer.height.unwrap_or(0.0),
        layer.texture_region,
        layer.transform,
    ))
}

fn native_vulkan_scene_opacity_effect_uv_space_from_snapshot_layer(
    layer: &SceneSnapshotLayer,
) -> Option<NativeVulkanSceneEffectUvSpace> {
    layer.alpha_texture_slot?;
    Some(native_vulkan_scene_effect_uv_space_from_transform(
        native_vulkan_scene_effect_uv_transform_for_scene_passes(
            &layer.image_effect_passes,
            layer.alpha_texture_slot,
        ),
        layer.width.unwrap_or(0.0),
        layer.height.unwrap_or(0.0),
        layer.texture_region,
        layer.transform,
    ))
}

pub(super) fn native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer(
    layer_index: usize,
    layer: &SceneSnapshotSampledImageLayer,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Result<Option<u32>, &'static str> {
    native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_uv_space(
        layer_index,
        layer,
        None,
        vertices,
    )
}

pub(super) fn native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_uv_space(
    layer_index: usize,
    layer: &SceneSnapshotSampledImageLayer,
    effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Result<Option<u32>, &'static str> {
    native_vulkan_scene_append_sampled_image_vertices_from_layer_parts(
        layer_index,
        SceneNodeKind::Image,
        layer.has_source,
        layer.fit,
        layer.opacity,
        layer.width,
        layer.height,
        layer.mesh.clone(),
        layer.effect_motion,
        layer.blend_mode,
        layer.tint,
        layer.texture_region,
        layer.transform,
        layer.alpha_texture_slot,
        layer.alpha_texture_mode.into(),
        effect_uv_space,
        vertices,
    )
}

pub(super) fn native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_chain(
    layer_index: usize,
    layer: &SceneSnapshotSampledImageLayer,
    effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Result<Option<u32>, &'static str> {
    if native_vulkan_scene_snapshot_first_class_effect_target_pass(&layer.image_effect_passes)
        .is_none()
    {
        return native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_uv_space(
            layer_index,
            layer,
            effect_uv_space,
            vertices,
        );
    }
    if layer.opacity <= 0.0 {
        return Ok(None);
    }
    if !layer.has_source {
        return Err("image-layer-missing-source");
    }
    let (width, height) = if layer.mesh.is_some() {
        (layer.width.unwrap_or(0.0), layer.height.unwrap_or(0.0))
    } else {
        (
            layer.width.ok_or("image-layer-missing-width")?,
            layer.height.ok_or("image-layer-missing-height")?,
        )
    };
    let texture_slots = native_vulkan_scene_texture_slots_from_scene_slots(&layer.texture_slots);
    let effect_passes =
        native_vulkan_scene_effect_passes_from_scene_passes(&layer.image_effect_passes);
    let alpha_texture_mode = layer.alpha_texture_mode.into();
    let material_pass = native_vulkan_scene_sampled_image_material_pass(
        NativeVulkanSceneMaterialKind::SampledImage,
        layer.blend_mode,
        layer.alpha_texture_slot,
        alpha_texture_mode,
        texture_slots.len(),
        &effect_passes,
    );
    let quad = NativeVulkanSceneSampledImageQuad {
        layer_index,
        layer_id: String::new(),
        source: PathBuf::new(),
        texture_slots,
        image_effect_pass_count: effect_passes.len(),
        effect_target_pass: native_vulkan_scene_snapshot_first_class_effect_target_pass(
            &layer.image_effect_passes,
        ),
        material_pass,
        base_blend_mode: layer.blend_mode,
        effect_passes,
        composite_key: layer.composite_key.clone(),
        fit: layer.fit,
        opacity: layer.opacity,
        tint: layer.tint,
        width,
        height,
        mesh: layer.mesh.clone(),
        effect_uv_space,
        effect_motion: layer.effect_motion,
        texture_region: layer.texture_region,
        transform: layer.transform,
    };
    if !native_vulkan_scene_sampled_image_quad_has_recordable_geometry(&quad) {
        return Err("image-layer-missing-recordable-geometry");
    }
    let first_vertex = vertices.len();
    let mut discarded_indices = Vec::new();
    native_vulkan_scene_append_sampled_image_effect_base_geometry(
        &quad,
        vertices,
        &mut discarded_indices,
    )
    .ok_or("image-layer-missing-recordable-geometry")?;
    native_vulkan_scene_append_sampled_image_effect_final_geometry(
        &quad,
        None,
        vertices,
        &mut discarded_indices,
    )
    .ok_or("image-layer-missing-recordable-geometry")?;
    Ok(Some(
        vertices
            .len()
            .saturating_sub(first_vertex)
            .min(u32::MAX as usize) as u32,
    ))
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn native_vulkan_scene_append_sampled_image_geometry_from_layer_parts(
    layer_index: usize,
    _layer_id: &str,
    kind: SceneNodeKind,
    has_source: bool,
    fit: FitMode,
    opacity: f64,
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<Arc<SceneMesh>>,
    effect_motion: SceneNativeEffectMotion,
    blend_mode: SceneBlendMode,
    tint: [f32; 4],
    texture_region: Option<SceneTextureRegion>,
    transform: SceneTransform,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneRenderAlphaTextureMode,
    effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Result<
    Option<(
        FitMode,
        Option<SceneTextureRegion>,
        NativeVulkanSceneSampledImageGeometryRange,
    )>,
    &'static str,
> {
    if opacity <= 0.0 {
        return Ok(None);
    }
    if kind != SceneNodeKind::Image {
        return Err("non-image-layer-needs-non-sampled-runtime");
    }
    if !has_source {
        return Err("image-layer-missing-source");
    }
    let (width, height) = if mesh.is_some() {
        (width.unwrap_or(0.0), height.unwrap_or(0.0))
    } else {
        (
            width.ok_or("image-layer-missing-width")?,
            height.ok_or("image-layer-missing-height")?,
        )
    };
    let effect_passes = Vec::new();
    let material_pass = native_vulkan_scene_sampled_image_material_pass(
        NativeVulkanSceneMaterialKind::SampledImage,
        blend_mode,
        alpha_texture_slot,
        alpha_texture_mode,
        0,
        &effect_passes,
    );
    let quad = NativeVulkanSceneSampledImageQuad {
        layer_index,
        layer_id: String::new(),
        source: PathBuf::new(),
        texture_slots: Vec::new(),
        image_effect_pass_count: 0,
        effect_target_pass: None,
        material_pass,
        base_blend_mode: blend_mode,
        effect_passes,
        composite_key: None,
        fit,
        opacity,
        tint,
        width,
        height,
        mesh,
        effect_uv_space,
        effect_motion,
        texture_region,
        transform,
    };
    let range = native_vulkan_scene_append_sampled_image_geometry(&quad, None, vertices, indices)
        .ok_or("image-layer-missing-recordable-geometry")?;
    Ok(Some((fit, texture_region, range)))
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn native_vulkan_scene_append_sampled_image_vertices_from_layer_parts(
    layer_index: usize,
    kind: SceneNodeKind,
    has_source: bool,
    fit: FitMode,
    opacity: f64,
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<Arc<SceneMesh>>,
    effect_motion: SceneNativeEffectMotion,
    blend_mode: SceneBlendMode,
    tint: [f32; 4],
    texture_region: Option<SceneTextureRegion>,
    transform: SceneTransform,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneRenderAlphaTextureMode,
    effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Result<Option<u32>, &'static str> {
    if opacity <= 0.0 {
        return Ok(None);
    }
    if kind != SceneNodeKind::Image {
        return Err("non-image-layer-needs-non-sampled-runtime");
    }
    if !has_source {
        return Err("image-layer-missing-source");
    }
    let (width, height) = if mesh.is_some() {
        (width.unwrap_or(0.0), height.unwrap_or(0.0))
    } else {
        (
            width.ok_or("image-layer-missing-width")?,
            height.ok_or("image-layer-missing-height")?,
        )
    };
    let effect_passes = Vec::new();
    let material_pass = native_vulkan_scene_sampled_image_material_pass(
        NativeVulkanSceneMaterialKind::SampledImage,
        blend_mode,
        alpha_texture_slot,
        alpha_texture_mode,
        0,
        &effect_passes,
    );
    let quad = NativeVulkanSceneSampledImageQuad {
        layer_index,
        layer_id: String::new(),
        source: PathBuf::new(),
        texture_slots: Vec::new(),
        image_effect_pass_count: 0,
        effect_target_pass: None,
        material_pass,
        base_blend_mode: blend_mode,
        effect_passes,
        composite_key: None,
        fit,
        opacity,
        tint,
        width,
        height,
        mesh,
        effect_uv_space,
        effect_motion,
        texture_region,
        transform,
    };
    native_vulkan_scene_append_sampled_image_vertices(&quad, vertices)
        .ok_or("image-layer-missing-recordable-geometry")
        .map(Some)
}

fn native_vulkan_scene_render_layer_has_shape_paint(layer: &SceneRenderLayer) -> bool {
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

fn native_vulkan_scene_render_layer_rectangle_kind(layer: &SceneRenderLayer) -> &'static str {
    if layer
        .corner_radius
        .is_some_and(|radius| radius.is_finite() && radius > 0.0)
    {
        "rounded-rectangle"
    } else {
        "rectangle"
    }
}

fn native_vulkan_scene_recordable_quad_from_render_layer(
    layer_index: usize,
    layer: &SceneRenderLayer,
    kind: &'static str,
) -> Option<NativeVulkanSceneRecordableQuad> {
    let opacity = layer.opacity.clamp(0.0, 1.0);
    let fill_color = layer
        .color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned);
    let fill_rgba = fill_color
        .as_deref()
        .and_then(|color| native_vulkan_scene_rgba_from_hex(color, opacity));
    let stroke_color = layer
        .stroke_color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned);
    let stroke_rgba = stroke_color
        .as_deref()
        .and_then(|color| native_vulkan_scene_rgba_from_hex(color, opacity));
    let stroke_width = stroke_rgba.map(|_| layer.stroke_width.unwrap_or(1.0));
    let (color, rgba) = fill_color
        .clone()
        .zip(fill_rgba)
        .or_else(|| stroke_color.clone().zip(stroke_rgba))?;
    Some(NativeVulkanSceneRecordableQuad {
        layer_index,
        layer_id: layer.id.clone(),
        kind,
        color,
        rgba,
        blend: native_vulkan_scene_blend_state(layer.blend_mode),
        fill_color,
        fill_rgba,
        stroke_color,
        stroke_rgba,
        stroke_width,
        width: layer.width,
        height: layer.height,
        corner_radius: layer.corner_radius,
        text: layer.text.clone(),
        font_size: layer.font_size,
        font_family: layer.font_family.clone(),
        font_source: layer.font_source.clone(),
        font_weight: layer.font_weight.clone(),
        text_align: layer.text_align,
        path_data: layer.path_data.clone(),
        path_fill_rule: layer.path_fill_rule,
        transform: layer.transform,
    })
}

#[derive(Debug)]
struct NativeVulkanSceneQuadRecordingPayload {
    steps: Vec<NativeVulkanSceneQuadRecordingStep>,
    vertices: Vec<NativeVulkanSceneQuadVertex>,
    indices: Vec<u32>,
}

#[derive(Debug)]
struct NativeVulkanSceneSampledImageRecordingPayload {
    sources: Vec<PathBuf>,
    effect_targets: Vec<NativeVulkanSceneSampledImageEffectTarget>,
    steps: Vec<NativeVulkanSceneSampledImageRecordingStep>,
    vertices: Vec<NativeVulkanSceneSampledImageVertex>,
    indices: Vec<u32>,
    recordable_quad_count: usize,
}

#[derive(Debug)]
struct NativeVulkanSceneVideoRecordingPayload {
    sources: Vec<PathBuf>,
    steps: Vec<NativeVulkanSceneVideoRecordingStep>,
    vertices: Vec<NativeVulkanSceneSampledImageVertex>,
    indices: Vec<u32>,
}

fn native_vulkan_scene_quad_recording_payload(
    quads: &[NativeVulkanSceneRecordableQuad],
) -> NativeVulkanSceneQuadRecordingPayload {
    let mut steps = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for quad in quads
        .iter()
        .filter(|quad| native_vulkan_scene_solid_has_recordable_geometry(quad))
    {
        if let Some((solid_vertices, solid_indices)) = native_vulkan_scene_solid_geometry(quad) {
            let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
            let first_index = indices.len().min(u32::MAX as usize) as u32;
            let vertex_count = solid_vertices.len().min(u32::MAX as usize) as u32;
            let index_count = solid_indices.len().min(u32::MAX as usize) as u32;
            steps.push(NativeVulkanSceneQuadRecordingStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                kind: quad.kind,
                blend: quad.blend,
                first_vertex,
                vertex_count,
                first_index,
                index_count,
                vertex_buffer_offset_bytes: u64::from(first_vertex)
                    .saturating_mul(SCENE_FULL_SOLID_QUAD_VERTEX_BYTES),
                vertex_buffer_size_bytes: u64::from(vertex_count)
                    .saturating_mul(SCENE_FULL_SOLID_QUAD_VERTEX_BYTES),
                index_buffer_offset_bytes: u64::from(first_index)
                    .saturating_mul(SCENE_FULL_SOLID_QUAD_INDEX_BYTES),
                index_buffer_size_bytes: u64::from(index_count)
                    .saturating_mul(SCENE_FULL_SOLID_QUAD_INDEX_BYTES),
                fill_geometry: quad.fill_rgba.is_some(),
                stroke_geometry: native_vulkan_scene_recordable_has_stroke_geometry(quad),
            });
            vertices.extend(solid_vertices);
            indices.extend(
                solid_indices
                    .into_iter()
                    .map(|index| first_vertex.saturating_add(index)),
            );
        }
    }
    NativeVulkanSceneQuadRecordingPayload {
        steps,
        vertices,
        indices,
    }
}

fn native_vulkan_scene_sampled_image_recording_payload(
    quads: &[NativeVulkanSceneSampledImageQuad],
    scene_size: Option<SceneSize>,
    we_graph_plan: &NativeVulkanSceneWeImageGraphPlan,
) -> NativeVulkanSceneSampledImageRecordingPayload {
    let mut sources = Vec::new();
    let recordable_quads = quads
        .iter()
        .filter(|quad| native_vulkan_scene_sampled_image_quad_has_recordable_geometry(quad))
        .collect::<Vec<_>>();
    let visible_quads = recordable_quads
        .iter()
        .copied()
        .filter(|quad| {
            native_vulkan_scene_sampled_image_quad_has_visible_recording_geometry(quad, scene_size)
        })
        .collect::<Vec<_>>();
    for quad in &visible_quads {
        let resource_index =
            native_vulkan_scene_sampled_image_source_index(&mut sources, quad.source.clone());
        let _ = native_vulkan_scene_sampled_image_material_texture_slot_bindings(
            &mut sources,
            quad,
            resource_index,
        );
        if native_vulkan_scene_sampled_image_needs_we_effect_chain(quad)
            && let Some(effect_pass) = quad.effect_target_pass.as_ref()
        {
            let _ = native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots(
                &mut sources,
                &effect_pass.texture_slots,
                resource_index,
            );
        }
    }
    let file_source_count = sources.len().max(1);
    let mut effect_targets = Vec::new();
    let mut steps = Vec::with_capacity(recordable_quads.len().saturating_mul(2));
    let mut vertices = Vec::with_capacity(recordable_quads.len().saturating_mul(4));
    let mut indices = Vec::with_capacity(recordable_quads.len().saturating_mul(6));
    let recordable_quad_count = recordable_quads.len();
    for quad in visible_quads {
        let resource_index =
            native_vulkan_scene_sampled_image_source_index(&mut sources, quad.source.clone());
        let texture_slot_bindings =
            native_vulkan_scene_sampled_image_material_texture_slot_bindings(
                &mut sources,
                quad,
                resource_index,
            );
        if native_vulkan_scene_sampled_image_needs_we_effect_chain(quad) {
            let scene_blend_mode = native_vulkan_scene_first_class_effect_scene_blend_mode(quad);
            let we_graph_target = native_vulkan_scene_we_image_graph_target_for_layer(
                we_graph_plan,
                quad.layer_index,
                NativeVulkanSceneWeImagePassEndpoint::FirstClassEffectTarget,
            );
            let we_graph_base_step = native_vulkan_scene_we_image_graph_step_for_layer_role(
                we_graph_plan,
                quad.layer_index,
                NativeVulkanSceneWeImagePassRole::BaseMaterial,
            );
            let we_graph_final_step = we_graph_target.and_then(|target| {
                native_vulkan_scene_we_image_graph_final_scene_step_for_layer(
                    we_graph_plan,
                    quad.layer_index,
                )
                .filter(|step| step.input_target_index == Some(target.target_index))
            });
            let Some(base_range) = native_vulkan_scene_append_sampled_image_effect_base_geometry(
                quad,
                &mut vertices,
                &mut indices,
            ) else {
                continue;
            };
            let target_index = effect_targets.len().min(u32::MAX as usize) as u32;
            effect_targets.push(NativeVulkanSceneSampledImageEffectTarget {
                effect_target_index: target_index,
                layer_index: quad.layer_index,
                width: native_vulkan_scene_effect_target_extent(quad.width),
                height: native_vulkan_scene_effect_target_extent(quad.height),
                we_graph_chain_index: we_graph_target.map(|target| target.chain_index),
                we_graph_target_index: we_graph_target.map(|target| target.target_index),
                we_graph_endpoint: we_graph_target.map(|target| target.endpoint),
            });
            let base_step = native_vulkan_scene_sampled_image_recording_step(
                quad,
                vec![NativeVulkanSceneTextureSlotResourceBinding {
                    slot: 0,
                    resource_index,
                }],
                None,
                SceneRenderAlphaTextureMode::Multiply,
                NativeVulkanSceneSampledImageRenderTarget::EffectTarget {
                    target_index,
                    clear: true,
                },
                we_graph_base_step,
                base_range,
                scene_blend_mode,
            );
            native_vulkan_scene_debug_sampled_image_recording_step(
                quad, &base_step, base_range, &vertices,
            );
            steps.push(base_step);

            let Some(final_range) = native_vulkan_scene_append_sampled_image_effect_final_geometry(
                quad,
                scene_size,
                &mut vertices,
                &mut indices,
            ) else {
                continue;
            };
            let target_resource_index = file_source_count
                .saturating_add(target_index as usize)
                .min(u32::MAX as usize) as u32;
            let (mut final_texture_slots, final_alpha_texture_slot, final_alpha_texture_mode) =
                if let Some(effect_pass) = quad.effect_target_pass.as_ref() {
                    (
                        native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots(
                            &mut sources,
                            &effect_pass.texture_slots,
                            target_resource_index,
                        ),
                        effect_pass.alpha_texture_slot,
                        effect_pass.alpha_texture_mode,
                    )
                } else {
                    (
                        vec![NativeVulkanSceneTextureSlotResourceBinding {
                            slot: 0,
                            resource_index: target_resource_index,
                        }],
                        quad.material_pass.alpha_texture_slot,
                        quad.material_pass.alpha_texture_mode,
                    )
                };
            if final_texture_slots.is_empty() {
                final_texture_slots.push(NativeVulkanSceneTextureSlotResourceBinding {
                    slot: 0,
                    resource_index: target_resource_index,
                });
            } else {
                final_texture_slots[0] = NativeVulkanSceneTextureSlotResourceBinding {
                    slot: 0,
                    resource_index: target_resource_index,
                };
            }
            if let Some(alpha_slot) = final_alpha_texture_slot {
                while final_texture_slots.len() <= alpha_slot as usize {
                    final_texture_slots.push(NativeVulkanSceneTextureSlotResourceBinding {
                        slot: final_texture_slots.len().min(u32::MAX as usize) as u32,
                        resource_index: target_resource_index,
                    });
                }
                if let Some(alpha_resource_index) = texture_slot_bindings
                    .iter()
                    .find(|binding| binding.slot == alpha_slot)
                    .map(|binding| binding.resource_index)
                {
                    final_texture_slots[alpha_slot as usize] =
                        NativeVulkanSceneTextureSlotResourceBinding {
                            slot: alpha_slot,
                            resource_index: alpha_resource_index,
                        };
                }
            }
            let final_step = native_vulkan_scene_sampled_image_recording_step(
                quad,
                final_texture_slots,
                final_alpha_texture_slot,
                final_alpha_texture_mode,
                NativeVulkanSceneSampledImageRenderTarget::Swapchain,
                we_graph_final_step,
                final_range,
                scene_blend_mode,
            );
            native_vulkan_scene_debug_sampled_image_recording_step(
                quad,
                &final_step,
                final_range,
                &vertices,
            );
            steps.push(final_step);
        } else if let Some(range) = native_vulkan_scene_append_sampled_image_geometry(
            quad,
            scene_size,
            &mut vertices,
            &mut indices,
        ) {
            let step = native_vulkan_scene_sampled_image_recording_step(
                quad,
                texture_slot_bindings,
                quad.material_pass.alpha_texture_slot,
                quad.material_pass.alpha_texture_mode,
                NativeVulkanSceneSampledImageRenderTarget::Swapchain,
                None,
                range,
                quad.material_pass.render_state.blend.mode,
            );
            native_vulkan_scene_debug_sampled_image_recording_step(quad, &step, range, &vertices);
            steps.push(step);
        }
    }
    NativeVulkanSceneSampledImageRecordingPayload {
        sources,
        effect_targets,
        steps,
        vertices,
        indices,
        recordable_quad_count,
    }
}

fn native_vulkan_scene_sampled_image_needs_we_effect_chain(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    quad.effect_target_pass.is_some()
}

fn native_vulkan_scene_we_image_graph_target_for_layer(
    plan: &NativeVulkanSceneWeImageGraphPlan,
    layer_index: usize,
    endpoint: NativeVulkanSceneWeImagePassEndpoint,
) -> Option<&NativeVulkanSceneWeImageGraphTarget> {
    plan.targets
        .iter()
        .find(|target| target.layer_index == layer_index && target.endpoint == endpoint)
}

fn native_vulkan_scene_we_image_graph_step_for_layer_role(
    plan: &NativeVulkanSceneWeImageGraphPlan,
    layer_index: usize,
    role: NativeVulkanSceneWeImagePassRole,
) -> Option<&NativeVulkanSceneWeImageGraphStep> {
    plan.steps
        .iter()
        .find(|step| step.layer_index == layer_index && step.pass.role == role)
}

fn native_vulkan_scene_we_image_graph_final_scene_step_for_layer(
    plan: &NativeVulkanSceneWeImageGraphPlan,
    layer_index: usize,
) -> Option<&NativeVulkanSceneWeImageGraphStep> {
    plan.steps
        .iter()
        .find(|step| step.layer_index == layer_index && step.pass.final_scene_pass)
}

fn native_vulkan_scene_first_class_effect_scene_blend_mode(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> SceneBlendMode {
    if matches!(quad.base_blend_mode, SceneBlendMode::Normal) {
        SceneBlendMode::Alpha
    } else {
        quad.base_blend_mode
    }
}

fn native_vulkan_scene_sampled_image_material_texture_slot_bindings(
    sources: &mut Vec<PathBuf>,
    quad: &NativeVulkanSceneSampledImageQuad,
    base_resource_index: u32,
) -> Vec<NativeVulkanSceneTextureSlotResourceBinding> {
    let texture_slots = native_vulkan_scene_sampled_image_material_texture_slots(quad);
    native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots(
        sources,
        &texture_slots,
        base_resource_index,
    )
}

fn native_vulkan_scene_sampled_image_material_texture_slots(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Vec<NativeVulkanSceneTextureSlot> {
    let mut texture_slots = quad.texture_slots.clone();
    for effect in &quad.effect_passes {
        for slot in &effect.texture_slots {
            if slot.slot == 0
                || texture_slots
                    .iter()
                    .any(|existing| existing.slot == slot.slot)
            {
                continue;
            }
            texture_slots.push(slot.clone());
        }
    }
    texture_slots.sort_by_key(|slot| slot.slot);
    texture_slots
}

fn native_vulkan_scene_sampled_image_quad_has_visible_recording_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    scene_size: Option<SceneSize>,
) -> bool {
    if !native_vulkan_scene_sampled_image_quad_has_recordable_geometry(quad) {
        return false;
    }
    if native_vulkan_scene_sampled_image_suppresses_unimplemented_we_effect_chain(quad) {
        return false;
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    if native_vulkan_scene_sampled_image_needs_we_effect_chain(quad) {
        native_vulkan_scene_append_sampled_image_effect_base_geometry(
            quad,
            &mut vertices,
            &mut indices,
        )
        .is_some()
            && native_vulkan_scene_append_sampled_image_effect_final_geometry(
                quad,
                scene_size,
                &mut vertices,
                &mut indices,
            )
            .is_some()
    } else {
        native_vulkan_scene_append_sampled_image_geometry(
            quad,
            scene_size,
            &mut vertices,
            &mut indices,
        )
        .is_some()
    }
}

fn native_vulkan_scene_effect_target_extent(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    value.ceil().clamp(1.0, u32::MAX as f64) as u32
}

fn native_vulkan_scene_sampled_image_suppresses_unimplemented_we_effect_chain(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    native_vulkan_scene_we_image_pass_chain(quad)
        .is_some_and(|chain| !chain.raw_direct_composite_allowed)
}

fn native_vulkan_scene_sampled_image_recording_step(
    quad: &NativeVulkanSceneSampledImageQuad,
    texture_slot_bindings: Vec<NativeVulkanSceneTextureSlotResourceBinding>,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneRenderAlphaTextureMode,
    render_target: NativeVulkanSceneSampledImageRenderTarget,
    we_graph_step: Option<&NativeVulkanSceneWeImageGraphStep>,
    range: NativeVulkanSceneSampledImageGeometryRange,
    blend_mode: SceneBlendMode,
) -> NativeVulkanSceneSampledImageRecordingStep {
    let material_kind = match render_target {
        NativeVulkanSceneSampledImageRenderTarget::EffectTarget { .. } => {
            NativeVulkanSceneMaterialKind::SampledImageEffectBase
        }
        NativeVulkanSceneSampledImageRenderTarget::Swapchain
            if native_vulkan_scene_sampled_image_needs_we_effect_chain(quad) =>
        {
            NativeVulkanSceneMaterialKind::SampledImageEffectComposite
        }
        NativeVulkanSceneSampledImageRenderTarget::Swapchain => {
            NativeVulkanSceneMaterialKind::SampledImage
        }
    };
    let material_pass = native_vulkan_scene_sampled_image_material_pass_with_effect_blend(
        material_kind,
        blend_mode,
        alpha_texture_slot,
        alpha_texture_mode,
        texture_slot_bindings.len(),
        &quad.effect_passes,
        false,
    );
    NativeVulkanSceneSampledImageRecordingStep {
        layer_index: quad.layer_index,
        layer_id: quad.layer_id.clone(),
        source: quad.source.clone(),
        fit: quad.fit,
        texture_region: quad.texture_region,
        texture_slot_bindings,
        material_pass,
        effect_passes: quad.effect_passes.clone(),
        composite_key: quad.composite_key.clone(),
        render_target,
        we_graph_chain_index: we_graph_step.map(|step| step.chain_index),
        we_graph_step_index: we_graph_step.map(|step| step.step_index),
        we_graph_input_target_index: we_graph_step.and_then(|step| step.input_target_index),
        we_graph_output_target_index: we_graph_step.and_then(|step| step.output_target_index),
        first_vertex: range.first_vertex,
        vertex_count: range.vertex_count,
        first_index: range.first_index,
        index_count: range.index_count,
        vertex_buffer_offset_bytes: u64::from(range.first_vertex)
            .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES),
        vertex_buffer_size_bytes: u64::from(range.vertex_count)
            .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES),
        index_buffer_offset_bytes: u64::from(range.first_index)
            .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES),
        index_buffer_size_bytes: u64::from(range.index_count)
            .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES),
    }
}

fn native_vulkan_scene_debug_sampled_image_recording_step(
    quad: &NativeVulkanSceneSampledImageQuad,
    step: &NativeVulkanSceneSampledImageRecordingStep,
    range: NativeVulkanSceneSampledImageGeometryRange,
    vertices: &[NativeVulkanSceneSampledImageVertex],
) {
    if !native_vulkan_effect_debug_enabled()
        || (step.material_pass.alpha_texture_slot.is_none()
            && quad.composite_key.is_none()
            && quad.effect_uv_space.is_none()
            && quad.effect_target_pass.is_none())
    {
        return;
    }
    native_vulkan_effect_debug_log(
        "draw-pass.sampled-step",
        format_args!(
            "layer_index={} layer_id={} source={} resource_index={} texture_slots={} effect_slots={} texture_slot_bindings={:?} alpha_slot={:?} mode={} blend={:?} material={} effects={} target={} geometry_semantics={} first_vertex={} vertex_count={} first_index={} index_count={} uv_range={} effect_uv_range={} effect_uv_space={}",
            step.layer_index,
            step.layer_id,
            step.source.display(),
            native_vulkan_scene_sampled_image_primary_resource_index(step)
                .map_or_else(|| "<missing>".to_owned(), |resource| resource.to_string()),
            native_vulkan_scene_debug_sampled_texture_slots_label(&quad.texture_slots),
            quad.effect_target_pass
                .as_ref()
                .map(
                    |pass| native_vulkan_scene_debug_sampled_texture_slots_label(
                        &pass.texture_slots
                    )
                )
                .unwrap_or_else(|| "[]".to_owned()),
            step.texture_slot_bindings,
            step.material_pass.alpha_texture_slot,
            step.material_pass.alpha_texture_mode.as_str(),
            step.material_pass.render_state.blend.mode,
            native_vulkan_scene_material_pass_label(&step.material_pass),
            native_vulkan_scene_effect_records_label(&step.effect_passes),
            native_vulkan_scene_sampled_image_render_target_label(step.render_target),
            native_vulkan_scene_sampled_image_recording_step_geometry_semantics_label(quad, step),
            step.first_vertex,
            step.vertex_count,
            step.first_index,
            step.index_count,
            native_vulkan_scene_debug_sampled_vertex_range_label(vertices, range, false),
            native_vulkan_scene_debug_sampled_vertex_range_label(vertices, range, true),
            native_vulkan_scene_debug_effect_uv_space_label(quad.effect_uv_space)
        ),
    );
    native_vulkan_scene_debug_sampled_alpha_mask(quad, &step.material_pass, range, vertices);
}

fn native_vulkan_scene_sampled_image_primary_resource_index(
    step: &NativeVulkanSceneSampledImageRecordingStep,
) -> Option<u32> {
    step.texture_slot_bindings
        .iter()
        .find(|binding| binding.slot == 0)
        .map(|binding| binding.resource_index)
}

fn native_vulkan_scene_sampled_image_render_target_label(
    target: NativeVulkanSceneSampledImageRenderTarget,
) -> String {
    match target {
        NativeVulkanSceneSampledImageRenderTarget::Swapchain => "swapchain".to_owned(),
        NativeVulkanSceneSampledImageRenderTarget::EffectTarget {
            target_index,
            clear,
        } => format!("effect-target(index={target_index}, clear={clear})"),
    }
}

fn native_vulkan_scene_debug_sampled_alpha_mask(
    quad: &NativeVulkanSceneSampledImageQuad,
    material: &NativeVulkanSceneMaterialPass,
    range: NativeVulkanSceneSampledImageGeometryRange,
    vertices: &[NativeVulkanSceneSampledImageVertex],
) {
    let Some(alpha_slot) = material.alpha_texture_slot else {
        return;
    };
    let Some(mask_slot) = quad
        .texture_slots
        .iter()
        .find(|slot| slot.slot == alpha_slot)
    else {
        native_vulkan_effect_debug_log(
            "draw-pass.alpha-mask",
            format_args!(
                "layer_index={} layer_id={} alpha_slot={} missing texture slot",
                quad.layer_index, quad.layer_id, alpha_slot
            ),
        );
        return;
    };
    let current_samples = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::CurrentEffect,
        true,
    );
    let current_coverage = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::CurrentEffect,
        false,
    );
    let base_uv_samples = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUv,
        true,
    );
    let base_uv_coverage = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUv,
        false,
    );
    let scene_bounds_samples = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBounds,
        true,
    );
    let scene_bounds_coverage = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBounds,
        false,
    );
    let bounds_scaled_samples = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBoundsAlphaOverBase,
        true,
    );
    let bounds_scaled_coverage = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBoundsAlphaOverBase,
        false,
    );
    let inverse_scale_samples = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUvBaseOverAlpha,
        true,
    );
    let inverse_scale_coverage = native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
        range,
        vertices,
        quad,
        material,
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUvBaseOverAlpha,
        false,
    );
    let groups = [
        NativeVulkanEffectDebugR8UvGroup {
            label: "current",
            sample_uvs: &current_samples,
            coverage_uvs: &current_coverage,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "base_uv",
            sample_uvs: &base_uv_samples,
            coverage_uvs: &base_uv_coverage,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "scene_bounds",
            sample_uvs: &scene_bounds_samples,
            coverage_uvs: &scene_bounds_coverage,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "bounds_scaled",
            sample_uvs: &bounds_scaled_samples,
            coverage_uvs: &bounds_scaled_coverage,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "base_over_alpha",
            sample_uvs: &inverse_scale_samples,
            coverage_uvs: &inverse_scale_coverage,
        },
    ];
    let vertex_label =
        native_vulkan_scene_debug_sampled_alpha_mask_probe_vertices_label(range, vertices);
    match native_vulkan_effect_debug_r8_gtex_group_report(&mask_slot.source, &groups) {
        Ok(report) => native_vulkan_effect_debug_log(
            "draw-pass.alpha-mask",
            format_args!(
                "layer_index={} layer_id={} alpha_slot={} mask_source={} base_slot={} uv_projection={} vertex_probes={} {}",
                quad.layer_index,
                quad.layer_id,
                alpha_slot,
                mask_slot.source.display(),
                native_vulkan_scene_debug_alpha_mask_base_slot_label(quad),
                native_vulkan_scene_debug_alpha_mask_uv_projection_label(quad, material),
                vertex_label,
                report
            ),
        ),
        Err(err) => native_vulkan_effect_debug_log(
            "draw-pass.alpha-mask",
            format_args!(
                "layer_index={} layer_id={} alpha_slot={} mask_source={} r8_report_error={} vertex_probes={}",
                quad.layer_index,
                quad.layer_id,
                alpha_slot,
                mask_slot.source.display(),
                err,
                vertex_label
            ),
        ),
    }
}

fn native_vulkan_scene_debug_sampled_alpha_mask_probe_uvs(
    range: NativeVulkanSceneSampledImageGeometryRange,
    vertices: &[NativeVulkanSceneSampledImageVertex],
    quad: &NativeVulkanSceneSampledImageQuad,
    material: &NativeVulkanSceneMaterialPass,
    projection: NativeVulkanSceneDebugAlphaMaskUvProjection,
    sparse: bool,
) -> Vec<[f32; 2]> {
    let mut probes = if sparse {
        vec![
            [0.0, 0.0],
            [0.25, 0.25],
            [0.5, 0.5],
            [0.75, 0.75],
            [1.0, 1.0],
        ]
    } else {
        Vec::new()
    };
    let start = range.first_vertex as usize;
    let count = range.vertex_count as usize;
    if count == 0 || start >= vertices.len() {
        return probes;
    }
    if sparse {
        let last = start
            .saturating_add(count.saturating_sub(1))
            .min(vertices.len() - 1);
        for offset in native_vulkan_scene_debug_alpha_mask_probe_offsets(count) {
            let index = start.saturating_add(offset).min(last);
            probes.push(native_vulkan_scene_debug_alpha_mask_project_uv(
                &vertices[index],
                quad,
                material,
                projection,
            ));
        }
    } else {
        let end = start.saturating_add(count).min(vertices.len());
        probes.reserve(end.saturating_sub(start));
        for vertex in &vertices[start..end] {
            probes.push(native_vulkan_scene_debug_alpha_mask_project_uv(
                vertex, quad, material, projection,
            ));
        }
    }
    probes
}

#[derive(Debug, Clone, Copy)]
enum NativeVulkanSceneDebugAlphaMaskUvProjection {
    CurrentEffect,
    BaseUv,
    SceneBounds,
    SceneBoundsAlphaOverBase,
    BaseUvBaseOverAlpha,
}

fn native_vulkan_scene_debug_alpha_mask_project_uv(
    vertex: &NativeVulkanSceneSampledImageVertex,
    quad: &NativeVulkanSceneSampledImageQuad,
    material: &NativeVulkanSceneMaterialPass,
    projection: NativeVulkanSceneDebugAlphaMaskUvProjection,
) -> [f32; 2] {
    match projection {
        NativeVulkanSceneDebugAlphaMaskUvProjection::CurrentEffect => vertex.effect_uv,
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUv => vertex.uv,
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBounds => {
            native_vulkan_scene_debug_alpha_mask_scene_bounds_uv(vertex, quad)
        }
        NativeVulkanSceneDebugAlphaMaskUvProjection::SceneBoundsAlphaOverBase => {
            let uv = native_vulkan_scene_debug_alpha_mask_scene_bounds_uv(vertex, quad);
            native_vulkan_scene_debug_alpha_mask_scale_uv(uv, quad, material, false)
        }
        NativeVulkanSceneDebugAlphaMaskUvProjection::BaseUvBaseOverAlpha => {
            native_vulkan_scene_debug_alpha_mask_scale_uv(vertex.uv, quad, material, true)
        }
    }
}

fn native_vulkan_scene_debug_alpha_mask_scene_bounds_uv(
    vertex: &NativeVulkanSceneSampledImageVertex,
    quad: &NativeVulkanSceneSampledImageQuad,
) -> [f32; 2] {
    let Some(space) = quad.effect_uv_space else {
        return vertex.effect_uv;
    };
    native_vulkan_scene_sampled_image_effect_uv_from_scene_position(
        vertex.position,
        NativeVulkanSceneEffectUvSpace {
            mapping: NativeVulkanSceneEffectUvMapping::ScenePositionBounds,
            ..space
        },
    )
}

fn native_vulkan_scene_debug_alpha_mask_scale_uv(
    uv: [f32; 2],
    quad: &NativeVulkanSceneSampledImageQuad,
    _material: &NativeVulkanSceneMaterialPass,
    inverse: bool,
) -> [f32; 2] {
    let Some((scale_u, scale_v, offset_u, offset_v)) =
        native_vulkan_scene_debug_alpha_mask_uv_transform(quad)
    else {
        return uv;
    };
    if inverse {
        return [
            if scale_u.abs() <= f32::EPSILON {
                uv[0]
            } else {
                (uv[0] - offset_u) / scale_u
            },
            if scale_v.abs() <= f32::EPSILON {
                uv[1]
            } else {
                (uv[1] - offset_v) / scale_v
            },
        ];
    }
    [
        uv[0].mul_add(scale_u, offset_u),
        uv[1].mul_add(scale_v, offset_v),
    ]
}

fn native_vulkan_scene_debug_alpha_mask_uv_transform(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<(f32, f32, f32, f32)> {
    let space = quad.effect_uv_space?;
    match space.mapping {
        NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
            scale_u,
            scale_v,
            offset_u,
            offset_v,
        } => Some((
            scale_u as f32,
            scale_v as f32,
            offset_u as f32,
            offset_v as f32,
        )),
        NativeVulkanSceneEffectUvMapping::ScenePositionBounds => None,
    }
}

fn native_vulkan_scene_debug_alpha_mask_probe_offsets(count: usize) -> [usize; 6] {
    [
        0,
        count / 8,
        count / 4,
        count / 2,
        count.saturating_mul(3) / 4,
        count.saturating_sub(1),
    ]
}

fn native_vulkan_scene_debug_alpha_mask_base_slot_label(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> String {
    quad.texture_slots
        .iter()
        .find(|slot| slot.slot == 0)
        .map(|slot| {
            format!(
                "{}{}",
                slot.source.display(),
                native_vulkan_scene_debug_texture_slot_extent_label(slot.width, slot.height)
            )
        })
        .unwrap_or_else(|| "<missing>".to_owned())
}

fn native_vulkan_scene_debug_alpha_mask_uv_projection_label(
    quad: &NativeVulkanSceneSampledImageQuad,
    _material: &NativeVulkanSceneMaterialPass,
) -> String {
    let transform = native_vulkan_scene_debug_alpha_mask_uv_transform(quad)
        .map(|(scale_u, scale_v, offset_u, offset_v)| {
            format!(
                " effect_uv_transform=scale:{scale_u:.6}/{scale_v:.6},offset:{offset_u:.6}/{offset_v:.6}"
            )
        })
        .unwrap_or_default();
    format!(
        "current={} alternatives=[base_uv, scene_bounds, bounds_scaled, inverse_transform]{}",
        native_vulkan_scene_debug_effect_uv_space_label(quad.effect_uv_space),
        transform
    )
}

fn native_vulkan_scene_debug_sampled_alpha_mask_probe_vertices_label(
    range: NativeVulkanSceneSampledImageGeometryRange,
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    let start = range.first_vertex as usize;
    let count = range.vertex_count as usize;
    if count == 0 || start >= vertices.len() {
        return "[]".to_owned();
    }
    let last = start
        .saturating_add(count.saturating_sub(1))
        .min(vertices.len() - 1);
    let mut label = String::new();
    label.push('[');
    for (probe_index, offset) in native_vulkan_scene_debug_alpha_mask_probe_offsets(count)
        .into_iter()
        .enumerate()
    {
        if probe_index > 0 {
            label.push_str(", ");
        }
        let index = start.saturating_add(offset).min(last);
        let vertex = &vertices[index];
        label.push_str(&format!(
            "{}:pos=({:.1},{:.1}) uv=({:.3},{:.3}) effect=({:.3},{:.3}) opacity={:.3}",
            index,
            vertex.position[0],
            vertex.position[1],
            vertex.uv[0],
            vertex.uv[1],
            vertex.effect_uv[0],
            vertex.effect_uv[1],
            vertex.opacity
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_debug_sampled_texture_slots_label(
    slots: &[NativeVulkanSceneTextureSlot],
) -> String {
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
            native_vulkan_scene_debug_texture_slot_extent_label(slot.width, slot.height)
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_debug_texture_slot_extent_label(
    width: Option<u32>,
    height: Option<u32>,
) -> String {
    match (width, height) {
        (Some(width), Some(height)) => format!("({width}x{height})"),
        _ => String::new(),
    }
}

fn native_vulkan_scene_debug_sampled_vertex_range_label(
    vertices: &[NativeVulkanSceneSampledImageVertex],
    range: NativeVulkanSceneSampledImageGeometryRange,
    effect_uv: bool,
) -> String {
    let start = range.first_vertex as usize;
    let end = start
        .saturating_add(range.vertex_count as usize)
        .min(vertices.len());
    if start >= end {
        return "<empty>".to_owned();
    }
    let mut min_u = f32::INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for vertex in &vertices[start..end] {
        let uv = if effect_uv {
            vertex.effect_uv
        } else {
            vertex.uv
        };
        min_u = min_u.min(uv[0]);
        min_v = min_v.min(uv[1]);
        max_u = max_u.max(uv[0]);
        max_v = max_v.max(uv[1]);
    }
    format!("u={min_u:.3}..{max_u:.3} v={min_v:.3}..{max_v:.3}")
}

fn native_vulkan_scene_debug_effect_uv_space_label(
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
        native_vulkan_scene_debug_effect_uv_mapping_label(space.mapping),
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

fn native_vulkan_scene_debug_effect_uv_mapping_label(
    mapping: NativeVulkanSceneEffectUvMapping,
) -> String {
    match mapping {
        NativeVulkanSceneEffectUvMapping::ScenePositionBounds => {
            "mapping=scene-position-bounds".to_owned()
        }
        NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
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

fn native_vulkan_scene_sampled_image_uses_we_effect_final_quad(
    _quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    false
}

fn native_vulkan_scene_sampled_image_recording_step_geometry_semantics_label(
    quad: &NativeVulkanSceneSampledImageQuad,
    step: &NativeVulkanSceneSampledImageRecordingStep,
) -> &'static str {
    if let Some(effect_pass) = quad.effect_target_pass.as_ref() {
        if matches!(
            effect_pass.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply | SceneRenderAlphaTextureMode::Coverage
        ) {
            return match step.render_target {
                NativeVulkanSceneSampledImageRenderTarget::EffectTarget { .. } => {
                    if quad.mesh.is_some() {
                        "we-opacity-effect-local-target-base-mesh"
                    } else {
                        "we-opacity-effect-local-target-base-quad"
                    }
                }
                NativeVulkanSceneSampledImageRenderTarget::Swapchain => {
                    "we-opacity-effect-final-scene-quad"
                }
            };
        }
        if matches!(
            effect_pass.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Iris
        ) {
            return match step.render_target {
                NativeVulkanSceneSampledImageRenderTarget::EffectTarget { .. } => {
                    if quad.mesh.is_some() {
                        "we-iris-effect-local-target-base-mesh"
                    } else {
                        "we-iris-effect-local-target-base-quad"
                    }
                }
                NativeVulkanSceneSampledImageRenderTarget::Swapchain => {
                    "we-iris-effect-final-scene-quad"
                }
            };
        }
    }
    native_vulkan_scene_sampled_image_geometry_semantics_label(quad, &step.material_pass)
}

fn native_vulkan_scene_sampled_image_geometry_semantics_label(
    quad: &NativeVulkanSceneSampledImageQuad,
    material: &NativeVulkanSceneMaterialPass,
) -> &'static str {
    if native_vulkan_scene_sampled_image_uses_we_effect_final_quad(quad) {
        "we-image-effect-chain-final-scene-quad"
    } else if material.alpha_texture_slot.is_some()
        && quad.mesh.is_some()
        && matches!(
            material.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Iris
        )
    {
        "we-iris-effect-direct-puppet-mesh-pass-space-raw-v"
    } else if material.alpha_texture_slot.is_some()
        && quad.mesh.is_some()
        && matches!(
            material.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply
        )
    {
        "we-opacity-effect-direct-puppet-mesh-material-uv"
    } else if quad.mesh.is_some() {
        "direct-puppet-mesh"
    } else {
        "direct-scene-quad"
    }
}

fn native_vulkan_scene_video_recording_payload(
    quads: &[NativeVulkanSceneVideoQuad],
) -> NativeVulkanSceneVideoRecordingPayload {
    let mut sources = Vec::new();
    let mut steps = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for quad in quads
        .iter()
        .filter(|quad| native_vulkan_scene_video_quad_has_recordable_geometry(quad))
    {
        if let Some(quad_vertices) = native_vulkan_scene_video_vertices(quad) {
            let resource_index =
                native_vulkan_scene_sampled_image_source_index(&mut sources, quad.source.clone());
            let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
            let first_index = indices.len().min(u32::MAX as usize) as u32;
            steps.push(NativeVulkanSceneVideoRecordingStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                source: quad.source.clone(),
                fit: quad.fit,
                pipeline: "decoded-video-layer-alpha-blend",
                resource_index,
                first_vertex,
                vertex_count: SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT,
                first_index,
                index_count: SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT,
                vertex_buffer_offset_bytes: u64::from(first_vertex)
                    .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES),
                vertex_buffer_size_bytes: u64::from(SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT)
                    .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES),
                index_buffer_offset_bytes: u64::from(first_index)
                    .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES),
                index_buffer_size_bytes: u64::from(SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT)
                    .saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES),
            });
            vertices.extend(quad_vertices);
            indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1,
                first_vertex + 2,
                first_vertex + 2,
                first_vertex + 1,
                first_vertex + 3,
            ]);
        }
    }
    NativeVulkanSceneVideoRecordingPayload {
        sources,
        steps,
        vertices,
        indices,
    }
}

pub(super) fn native_vulkan_scene_sampled_image_vertices_visible_in_scene(
    vertices: &[NativeVulkanSceneSampledImageVertex],
    scene_size: Option<SceneSize>,
) -> bool {
    let Some(scene_size) = scene_size else {
        return true;
    };
    if scene_size.width == 0 || scene_size.height == 0 || vertices.is_empty() {
        return true;
    }
    let Some(bounds) = NativeVulkanSceneSampledImageBounds::from_vertices(vertices) else {
        return false;
    };
    bounds.intersects_scene(scene_size)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NativeVulkanSceneSampledImageBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl NativeVulkanSceneSampledImageBounds {
    fn from_vertices(vertices: &[NativeVulkanSceneSampledImageVertex]) -> Option<Self> {
        let mut bounds = Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
        };
        for vertex in vertices {
            let [x, y] = vertex.position;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            bounds.min_x = bounds.min_x.min(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_x = bounds.max_x.max(x);
            bounds.max_y = bounds.max_y.max(y);
        }
        Some(bounds)
    }

    fn intersects_scene(self, scene_size: SceneSize) -> bool {
        let scene_width = scene_size.width as f32;
        let scene_height = scene_size.height as f32;
        self.max_x >= 0.0
            && self.max_y >= 0.0
            && self.min_x <= scene_width
            && self.min_y <= scene_height
    }
}

fn native_vulkan_scene_solid_has_recordable_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> bool {
    match quad.kind {
        "rectangle" | "rounded-rectangle" | "audio-response" => {
            quad.width
                .is_some_and(|width| width.is_finite() && width > 0.0)
                && quad
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
                && (quad.fill_rgba.is_some()
                    || native_vulkan_scene_recordable_has_stroke_geometry(quad))
        }
        "ellipse" => {
            quad.width
                .is_some_and(|width| width.is_finite() && width > 0.0)
                && quad
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
                && (quad.fill_rgba.is_some()
                    || native_vulkan_scene_recordable_has_stroke_geometry(quad))
        }
        "path" => {
            quad.path_data
                .as_deref()
                .is_some_and(|path| !path.is_empty())
                && (quad.fill_rgba.is_some()
                    || native_vulkan_scene_recordable_has_stroke_geometry(quad))
        }
        "text" => {
            quad.text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
                && native_vulkan_scene_text_font_size(quad).is_some()
        }
        _ => false,
    }
}

fn native_vulkan_scene_recordable_has_stroke_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> bool {
    quad.stroke_rgba.is_some()
        && quad
            .stroke_width
            .is_some_and(|width| width.is_finite() && width > 0.0)
}

fn native_vulkan_scene_sampled_image_quad_has_recordable_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    quad.opacity.is_finite()
        && quad.opacity > 0.0
        && if let Some(mesh) = &quad.mesh {
            quad.width.is_finite()
                && quad.width > 0.0
                && quad.height.is_finite()
                && quad.height > 0.0
                && mesh.vertices.len() >= 3
                && mesh.indices.len() >= 3
                && mesh.indices.len() % 3 == 0
        } else {
            quad.width.is_finite()
                && quad.width > 0.0
                && quad.height.is_finite()
                && quad.height > 0.0
        }
}

fn native_vulkan_scene_video_quad_has_recordable_geometry(
    quad: &NativeVulkanSceneVideoQuad,
) -> bool {
    quad.width.is_finite()
        && quad.width > 0.0
        && quad.height.is_finite()
        && quad.height > 0.0
        && quad.opacity.is_finite()
        && quad.opacity > 0.0
}

fn native_vulkan_scene_solid_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    match quad.kind {
        "rectangle" | "audio-response" => native_vulkan_scene_rectangle_geometry(quad),
        "rounded-rectangle" => native_vulkan_scene_rounded_rectangle_geometry(quad),
        "ellipse" => native_vulkan_scene_ellipse_geometry(quad),
        "path" => native_vulkan_scene_path_geometry(quad),
        "text" => native_vulkan_scene_text_geometry(quad),
        _ => None,
    }
}

fn native_vulkan_scene_rectangle_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    let width = quad.width?;
    let height = quad.height?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let left = -quad.transform.anchor_x * width;
    let top = -quad.transform.anchor_y * height;
    let right = left + width;
    let bottom = top + height;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    if let Some(fill_rgba) = quad.fill_rgba {
        native_vulkan_scene_push_solid_rect(
            &mut vertices,
            &mut indices,
            left,
            top,
            width,
            height,
            fill_rgba,
            quad.transform,
        )?;
    }
    if let (Some(stroke_rgba), Some(stroke_width)) = (quad.stroke_rgba, quad.stroke_width) {
        native_vulkan_scene_push_rect_stroke(
            &mut vertices,
            &mut indices,
            left,
            top,
            right,
            bottom,
            stroke_width,
            stroke_rgba,
            quad.transform,
        )?;
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_rounded_rectangle_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    let width = quad.width?;
    let height = quad.height?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let radius = quad.corner_radius?.clamp(0.0, width.min(height) * 0.5);
    if !radius.is_finite() || radius <= 0.0 {
        return native_vulkan_scene_rectangle_geometry(quad);
    }

    let left = -quad.transform.anchor_x * width;
    let top = -quad.transform.anchor_y * height;
    let right = left + width;
    let bottom = top + height;
    let outline = native_vulkan_scene_rounded_rectangle_outline(left, top, right, bottom, radius);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    if let Some(fill_rgba) = quad.fill_rgba {
        native_vulkan_scene_push_polygon_fan(
            &mut vertices,
            &mut indices,
            &outline,
            [(left + right) * 0.5, (top + bottom) * 0.5],
            fill_rgba,
            quad.transform,
        )?;
    }
    if let (Some(stroke_rgba), Some(stroke_width)) = (quad.stroke_rgba, quad.stroke_width) {
        let half_extent = width.min(height) * 0.5;
        let stroke_width = stroke_width.clamp(0.0, half_extent);
        if stroke_width > 0.0 {
            let inner_left = left + stroke_width;
            let inner_top = top + stroke_width;
            let inner_right = right - stroke_width;
            let inner_bottom = bottom - stroke_width;
            if inner_left < inner_right && inner_top < inner_bottom {
                let inner_radius = (radius - stroke_width).max(0.0);
                let inner_outline = native_vulkan_scene_rounded_rectangle_outline(
                    inner_left,
                    inner_top,
                    inner_right,
                    inner_bottom,
                    inner_radius,
                );
                native_vulkan_scene_push_outline_ring(
                    &mut vertices,
                    &mut indices,
                    &outline,
                    &inner_outline,
                    stroke_rgba,
                    quad.transform,
                )?;
            } else {
                native_vulkan_scene_push_polygon_fan(
                    &mut vertices,
                    &mut indices,
                    &outline,
                    [(left + right) * 0.5, (top + bottom) * 0.5],
                    stroke_rgba,
                    quad.transform,
                )?;
            }
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_ellipse_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    let width = quad.width?;
    let height = quad.height?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }

    let left = -quad.transform.anchor_x * width;
    let top = -quad.transform.anchor_y * height;
    let center_x = left + width * 0.5;
    let center_y = top + height * 0.5;
    let radius_x = width * 0.5;
    let radius_y = height * 0.5;
    let outline = native_vulkan_scene_ellipse_outline(center_x, center_y, radius_x, radius_y);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    if let Some(fill_rgba) = quad.fill_rgba {
        native_vulkan_scene_push_polygon_fan(
            &mut vertices,
            &mut indices,
            &outline,
            [center_x, center_y],
            fill_rgba,
            quad.transform,
        )?;
    }
    if let (Some(stroke_rgba), Some(stroke_width)) = (quad.stroke_rgba, quad.stroke_width) {
        let stroke_width = stroke_width.clamp(0.0, radius_x.min(radius_y));
        if stroke_width > 0.0 {
            let inner_radius_x = radius_x - stroke_width;
            let inner_radius_y = radius_y - stroke_width;
            if inner_radius_x > 0.0 && inner_radius_y > 0.0 {
                let inner_outline = native_vulkan_scene_ellipse_outline(
                    center_x,
                    center_y,
                    inner_radius_x,
                    inner_radius_y,
                );
                native_vulkan_scene_push_outline_ring(
                    &mut vertices,
                    &mut indices,
                    &outline,
                    &inner_outline,
                    stroke_rgba,
                    quad.transform,
                )?;
            } else {
                native_vulkan_scene_push_polygon_fan(
                    &mut vertices,
                    &mut indices,
                    &outline,
                    [center_x, center_y],
                    stroke_rgba,
                    quad.transform,
                )?;
            }
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_ellipse_outline(
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
) -> Vec<[f64; 2]> {
    let mut outline = Vec::with_capacity(SCENE_FULL_ELLIPSE_SEGMENTS);
    for segment in 0..SCENE_FULL_ELLIPSE_SEGMENTS {
        let theta = (segment as f64) * std::f64::consts::TAU / (SCENE_FULL_ELLIPSE_SEGMENTS as f64);
        outline.push([
            center_x + theta.cos() * radius_x,
            center_y + theta.sin() * radius_y,
        ]);
    }
    outline
}

fn native_vulkan_scene_path_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    let subpaths = native_vulkan_scene_path_subpaths(quad.path_data.as_deref()?)?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    if let Some(fill_rgba) = quad.fill_rgba {
        let fill_subpaths = subpaths
            .iter()
            .filter(|subpath| subpath.points.len() >= 3)
            .collect::<Vec<_>>();
        if fill_subpaths.len() == 1 {
            native_vulkan_scene_push_path_fill(
                &mut vertices,
                &mut indices,
                &fill_subpaths[0].points,
                fill_rgba,
                quad.transform,
            )?;
        } else if fill_subpaths.len() > 1 {
            native_vulkan_scene_push_compound_path_fill(
                &mut vertices,
                &mut indices,
                &fill_subpaths,
                quad.path_fill_rule,
                fill_rgba,
                quad.transform,
            )?;
        }
    }
    if let (Some(stroke_rgba), Some(stroke_width)) = (quad.stroke_rgba, quad.stroke_width) {
        for subpath in subpaths.iter().filter(|subpath| subpath.points.len() >= 2) {
            native_vulkan_scene_push_polyline_stroke(
                &mut vertices,
                &mut indices,
                &subpath.points,
                subpath.closed,
                stroke_width,
                stroke_rgba,
                quad.transform,
            )?;
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_push_path_fill(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    points: &[[f64; 2]],
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if points.len() < 3 {
        return None;
    }
    let local_indices = if native_vulkan_scene_polygon_is_convex(points) {
        let mut indices = Vec::with_capacity((points.len().saturating_sub(2)) * 3);
        for index in 1..points.len().saturating_sub(1) {
            indices.extend_from_slice(&[0, index as u32, index as u32 + 1]);
        }
        indices
    } else {
        native_vulkan_scene_triangulate_simple_polygon(points)?
    };
    if vertices.len().saturating_add(points.len()) > u32::MAX as usize {
        return None;
    }
    let first_vertex = vertices.len() as u32;
    vertices.extend(
        points
            .iter()
            .map(|[x, y]| {
                Some(NativeVulkanSceneQuadVertex {
                    position: native_vulkan_scene_transform_point(*x, *y, transform)?,
                    rgba,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    );
    indices.extend(
        local_indices
            .into_iter()
            .map(|index| first_vertex.saturating_add(index)),
    );
    Some(())
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanScenePathFillEdge {
    start: [f64; 2],
    end: [f64; 2],
    winding: i32,
}

impl NativeVulkanScenePathFillEdge {
    fn contains_y(self, y: f64) -> bool {
        let min_y = self.start[1].min(self.end[1]);
        let max_y = self.start[1].max(self.end[1]);
        y > min_y && y < max_y
    }

    fn x_at_y(self, y: f64) -> Option<f64> {
        let dy = self.end[1] - self.start[1];
        if dy.abs() <= f64::EPSILON || !dy.is_finite() {
            return None;
        }
        let t = (y - self.start[1]) / dy;
        let x = self.start[0] + (self.end[0] - self.start[0]) * t;
        x.is_finite().then_some(x)
    }
}

fn native_vulkan_scene_push_compound_path_fill(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    subpaths: &[&NativeVulkanScenePathSubpath],
    fill_rule: ScenePathFillRule,
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    let mut edges = Vec::new();
    let mut y_values = Vec::new();
    for subpath in subpaths {
        y_values.extend(subpath.points.iter().map(|point| point[1]));
        for index in 0..subpath.points.len() {
            let start = subpath.points[index];
            let end = subpath.points[(index + 1) % subpath.points.len()];
            if !native_vulkan_scene_path_points_close(start, end)
                && (start[1] - end[1]).abs() > SCENE_FULL_PATH_POINT_EPSILON
            {
                edges.push(NativeVulkanScenePathFillEdge {
                    start,
                    end,
                    winding: if end[1] > start[1] { 1 } else { -1 },
                });
            }
        }
    }
    if edges.is_empty() {
        return Some(());
    }
    y_values.retain(|value| value.is_finite());
    y_values.sort_by(|left, right| left.total_cmp(right));
    y_values.dedup_by(|left, right| (*left - *right).abs() <= SCENE_FULL_PATH_POINT_EPSILON);

    for band in y_values.windows(2) {
        let top = band[0];
        let bottom = band[1];
        if bottom - top <= SCENE_FULL_PATH_POINT_EPSILON {
            continue;
        }
        let mid_y = (top + bottom) * 0.5;
        let mut intersections = edges
            .iter()
            .filter(|edge| edge.contains_y(mid_y))
            .filter_map(|edge| Some((*edge, edge.x_at_y(mid_y)?)))
            .collect::<Vec<_>>();
        if intersections.is_empty() {
            continue;
        }
        intersections.sort_by(|left, right| left.1.total_cmp(&right.1));
        match fill_rule {
            ScenePathFillRule::Evenodd => {
                if intersections.len() % 2 != 0 {
                    return None;
                }
                for pair in intersections.chunks_exact(2) {
                    native_vulkan_scene_push_path_fill_span(
                        vertices, indices, pair[0], pair[1], top, bottom, rgba, transform,
                    )?;
                }
            }
            ScenePathFillRule::Nonzero => {
                let mut winding = 0i32;
                let mut span_start: Option<(NativeVulkanScenePathFillEdge, f64)> = None;
                for intersection in intersections {
                    let previous = winding;
                    winding += intersection.0.winding;
                    if previous == 0 && winding != 0 {
                        span_start = Some(intersection);
                    } else if previous != 0 && winding == 0 {
                        let start = span_start.take()?;
                        native_vulkan_scene_push_path_fill_span(
                            vertices,
                            indices,
                            start,
                            intersection,
                            top,
                            bottom,
                            rgba,
                            transform,
                        )?;
                    }
                }
                if span_start.is_some() || winding != 0 {
                    return None;
                }
            }
        }
    }
    Some(())
}

fn native_vulkan_scene_push_path_fill_span(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    left: (NativeVulkanScenePathFillEdge, f64),
    right: (NativeVulkanScenePathFillEdge, f64),
    top: f64,
    bottom: f64,
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if (right.1 - left.1).abs() <= SCENE_FULL_PATH_POINT_EPSILON {
        return Some(());
    }
    let left_top = left.0.x_at_y(top)?;
    let right_top = right.0.x_at_y(top)?;
    let left_bottom = left.0.x_at_y(bottom)?;
    let right_bottom = right.0.x_at_y(bottom)?;
    native_vulkan_scene_push_solid_quad_points(
        vertices,
        indices,
        [
            [left_top, top],
            [right_top, top],
            [left_bottom, bottom],
            [right_bottom, bottom],
        ],
        rgba,
        transform,
    )
}

fn native_vulkan_scene_push_rect_stroke(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    stroke_width: f64,
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if !stroke_width.is_finite() || stroke_width <= 0.0 {
        return Some(());
    }
    let stroke_width = stroke_width
        .min((right - left).abs() * 0.5)
        .min((bottom - top).abs() * 0.5);
    if stroke_width <= 0.0 {
        return Some(());
    }
    native_vulkan_scene_push_solid_rect(
        vertices,
        indices,
        left,
        top,
        right - left,
        stroke_width,
        rgba,
        transform,
    )?;
    native_vulkan_scene_push_solid_rect(
        vertices,
        indices,
        left,
        bottom - stroke_width,
        right - left,
        stroke_width,
        rgba,
        transform,
    )?;
    let side_height = bottom - top - stroke_width * 2.0;
    if side_height > 0.0 {
        native_vulkan_scene_push_solid_rect(
            vertices,
            indices,
            left,
            top + stroke_width,
            stroke_width,
            side_height,
            rgba,
            transform,
        )?;
        native_vulkan_scene_push_solid_rect(
            vertices,
            indices,
            right - stroke_width,
            top + stroke_width,
            stroke_width,
            side_height,
            rgba,
            transform,
        )?;
    }
    Some(())
}

fn native_vulkan_scene_rounded_rectangle_outline(
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    radius: f64,
) -> Vec<[f64; 2]> {
    let radius = radius.clamp(0.0, ((right - left).abs()).min((bottom - top).abs()) * 0.5);
    if radius <= 0.0 {
        return vec![[left, top], [right, top], [right, bottom], [left, bottom]];
    }
    let corners = [
        (
            right - radius,
            top + radius,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        ),
        (
            right - radius,
            bottom - radius,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ),
        (
            left + radius,
            bottom - radius,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        ),
        (
            left + radius,
            top + radius,
            std::f64::consts::PI,
            std::f64::consts::PI * 1.5,
        ),
    ];
    let mut outline = Vec::with_capacity((SCENE_FULL_ROUNDED_RECT_CORNER_SEGMENTS + 1) * 4);
    for (center_x, center_y, start_angle, end_angle) in corners {
        for segment in 0..=SCENE_FULL_ROUNDED_RECT_CORNER_SEGMENTS {
            let t = segment as f64 / SCENE_FULL_ROUNDED_RECT_CORNER_SEGMENTS as f64;
            let angle = start_angle + (end_angle - start_angle) * t;
            outline.push([
                center_x + angle.cos() * radius,
                center_y + angle.sin() * radius,
            ]);
        }
    }
    outline
}

fn native_vulkan_scene_push_polygon_fan(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    outline: &[[f64; 2]],
    center: [f64; 2],
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if outline.len() < 3 || vertices.len().saturating_add(outline.len() + 1) > u32::MAX as usize {
        return None;
    }
    let center_vertex = vertices.len() as u32;
    vertices.push(NativeVulkanSceneQuadVertex {
        position: native_vulkan_scene_transform_point(center[0], center[1], transform)?,
        rgba,
    });
    vertices.extend(
        outline
            .iter()
            .map(|[x, y]| {
                Some(NativeVulkanSceneQuadVertex {
                    position: native_vulkan_scene_transform_point(*x, *y, transform)?,
                    rgba,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    );
    for index in 0..outline.len() {
        let current = center_vertex + index as u32 + 1;
        let next = if index + 1 == outline.len() {
            center_vertex + 1
        } else {
            current + 1
        };
        indices.extend_from_slice(&[center_vertex, current, next]);
    }
    Some(())
}

fn native_vulkan_scene_push_outline_ring(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    outer: &[[f64; 2]],
    inner: &[[f64; 2]],
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if outer.len() < 3
        || outer.len() != inner.len()
        || vertices.len().saturating_add(outer.len() * 2) > u32::MAX as usize
    {
        return None;
    }
    let first_outer = vertices.len() as u32;
    vertices.extend(
        outer
            .iter()
            .map(|[x, y]| {
                Some(NativeVulkanSceneQuadVertex {
                    position: native_vulkan_scene_transform_point(*x, *y, transform)?,
                    rgba,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    );
    let first_inner = vertices.len() as u32;
    vertices.extend(
        inner
            .iter()
            .map(|[x, y]| {
                Some(NativeVulkanSceneQuadVertex {
                    position: native_vulkan_scene_transform_point(*x, *y, transform)?,
                    rgba,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    );
    for index in 0..outer.len() {
        let next = if index + 1 == outer.len() {
            0
        } else {
            index + 1
        };
        let outer_current = first_outer + index as u32;
        let outer_next = first_outer + next as u32;
        let inner_current = first_inner + index as u32;
        let inner_next = first_inner + next as u32;
        indices.extend_from_slice(&[
            outer_current,
            outer_next,
            inner_current,
            inner_current,
            outer_next,
            inner_next,
        ]);
    }
    Some(())
}

fn native_vulkan_scene_push_polyline_stroke(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    points: &[[f64; 2]],
    closed: bool,
    stroke_width: f64,
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if points.len() < 2 || !stroke_width.is_finite() || stroke_width <= 0.0 {
        return Some(());
    }
    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let half_width = stroke_width * 0.5;
    for index in 0..segment_count {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy);
        if length <= f64::EPSILON || !length.is_finite() {
            continue;
        }
        let nx = -dy / length * half_width;
        let ny = dx / length * half_width;
        native_vulkan_scene_push_solid_quad_points(
            vertices,
            indices,
            [
                [a[0] + nx, a[1] + ny],
                [b[0] + nx, b[1] + ny],
                [a[0] - nx, a[1] - ny],
                [b[0] - nx, b[1] - ny],
            ],
            rgba,
            transform,
        )?;
    }
    Some(())
}

fn native_vulkan_scene_text_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneQuadVertex>, Vec<u32>)> {
    let text = quad.text.as_deref()?.trim_end_matches(['\r', '\n']);
    if text.trim().is_empty() {
        return None;
    }
    let font_size = native_vulkan_scene_text_font_size(quad)?;
    let cell = font_size / SCENE_FULL_TEXT_GLYPH_ROWS as f64;
    let line_advance = cell * SCENE_FULL_TEXT_LINE_ADVANCE_ROWS;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let measured_width = lines
        .iter()
        .map(|line| native_vulkan_scene_text_line_width(line, cell))
        .fold(0.0, f64::max);
    if measured_width <= 0.0 {
        return None;
    }
    let layout_width = quad
        .width
        .filter(|width| width.is_finite() && *width > 0.0)
        .unwrap_or(measured_width);
    let measured_height = font_size + line_advance * lines.len().saturating_sub(1) as f64;
    let layout_height = quad
        .height
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(measured_height);
    let left = -quad.transform.anchor_x * layout_width;
    let top = -quad.transform.anchor_y * layout_height;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let line_width = native_vulkan_scene_text_line_width(line, cell);
        let align_offset = match quad.text_align.unwrap_or_default() {
            SceneTextAlign::Start => 0.0,
            SceneTextAlign::Middle => (layout_width - line_width) * 0.5,
            SceneTextAlign::End => layout_width - line_width,
        };
        let mut cursor_x = left + align_offset.max(0.0);
        let line_top = top + line_index as f64 * line_advance;
        for ch in line.chars() {
            let pattern = native_vulkan_scene_text_glyph_pattern(ch);
            for (row, bits) in pattern.iter().enumerate() {
                for column in 0..SCENE_FULL_TEXT_GLYPH_COLUMNS {
                    let mask = 1u8 << (SCENE_FULL_TEXT_GLYPH_COLUMNS - 1 - column);
                    if bits & mask == 0 {
                        continue;
                    }
                    native_vulkan_scene_push_solid_rect(
                        &mut vertices,
                        &mut indices,
                        cursor_x + column as f64 * cell,
                        line_top + row as f64 * cell,
                        cell,
                        cell,
                        quad.rgba,
                        quad.transform,
                    )?;
                }
            }
            cursor_x += cell * SCENE_FULL_TEXT_GLYPH_ADVANCE_COLUMNS;
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_text_font_size(quad: &NativeVulkanSceneRecordableQuad) -> Option<f64> {
    let font_size = quad.font_size.unwrap_or(SCENE_FULL_TEXT_DEFAULT_FONT_SIZE);
    if font_size.is_finite() && font_size > 0.0 {
        Some(font_size)
    } else {
        None
    }
}

fn native_vulkan_scene_text_line_width(line: &str, cell: f64) -> f64 {
    let char_count = line.chars().count();
    if char_count == 0 {
        0.0
    } else {
        let columns = SCENE_FULL_TEXT_GLYPH_COLUMNS as f64
            + SCENE_FULL_TEXT_GLYPH_ADVANCE_COLUMNS * char_count.saturating_sub(1) as f64;
        columns * cell
    }
}

fn native_vulkan_scene_push_solid_rect(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    native_vulkan_scene_push_solid_quad_points(
        vertices,
        indices,
        [
            [x, y],
            [x + width, y],
            [x, y + height],
            [x + width, y + height],
        ],
        rgba,
        transform,
    )
}

fn native_vulkan_scene_push_solid_quad_points(
    vertices: &mut Vec<NativeVulkanSceneQuadVertex>,
    indices: &mut Vec<u32>,
    points: [[f64; 2]; 4],
    rgba: [f32; 4],
    transform: SceneTransform,
) -> Option<()> {
    if vertices.len().saturating_add(4) > u32::MAX as usize {
        return None;
    }
    let first_vertex = vertices.len() as u32;
    for [x, y] in points {
        vertices.push(NativeVulkanSceneQuadVertex {
            position: native_vulkan_scene_transform_point(x, y, transform)?,
            rgba,
        });
    }
    indices.extend_from_slice(&[
        first_vertex,
        first_vertex + 1,
        first_vertex + 2,
        first_vertex + 2,
        first_vertex + 1,
        first_vertex + 3,
    ]);
    Some(())
}

fn native_vulkan_scene_text_glyph_pattern(ch: char) -> [u8; SCENE_FULL_TEXT_GLYPH_ROWS] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ' ' | '\t' => [0, 0, 0, 0, 0, 0, 0],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ',' => [0, 0, 0, 0, 0, 0b01100, 0b01000],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '#' => [
            0b01010, 0b11111, 0b01010, 0b01010, 0b11111, 0b01010, 0b01010,
        ],
        '%' => [
            0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
        ],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        _ => [0b11111, 0b00001, 0b00010, 0b00100, 0b00100, 0, 0b00100],
    }
}

fn native_vulkan_scene_sampled_image_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<(Vec<NativeVulkanSceneSampledImageVertex>, Vec<u32>)> {
    if native_vulkan_scene_sampled_image_uses_we_effect_final_quad(quad) {
        let vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
        return Some((vertices.to_vec(), vec![0, 1, 2, 2, 1, 3]));
    }
    if let Some(mesh) = &quad.mesh {
        if quad.effect_motion.is_active() {
            if let Some(corners) = native_vulkan_scene_sampled_image_mesh_grid_corners(quad, mesh) {
                let mut vertices = Vec::new();
                let segments = sampled_image_motion::grid_segments(
                    quad.width,
                    quad.height,
                    quad.effect_motion,
                );
                native_vulkan_scene_append_sampled_image_effect_grid_vertices(
                    quad,
                    corners,
                    segments,
                    &mut vertices,
                )?;
                let mut indices = Vec::new();
                native_vulkan_scene_append_sampled_image_effect_grid_indices(
                    0,
                    segments,
                    &mut indices,
                )?;
                return Some((vertices, indices));
            }
            return native_vulkan_scene_sampled_image_subdivided_mesh_geometry(quad, mesh);
        }
        return native_vulkan_scene_sampled_image_mesh_geometry(quad, mesh);
    }
    if quad.effect_motion.is_active() {
        let corners = native_vulkan_scene_sampled_image_quad_grid_corners(quad)?;
        let segments =
            sampled_image_motion::grid_segments(quad.width, quad.height, quad.effect_motion);
        let mut vertices = Vec::new();
        native_vulkan_scene_append_sampled_image_effect_grid_vertices(
            quad,
            corners,
            segments,
            &mut vertices,
        )?;
        let mut indices = Vec::new();
        native_vulkan_scene_append_sampled_image_effect_grid_indices(0, segments, &mut indices)?;
        return Some((vertices, indices));
    }
    let vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
    Some((vertices.to_vec(), vec![0, 1, 2, 2, 1, 3]))
}

fn native_vulkan_scene_append_sampled_image_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    scene_size: Option<SceneSize>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<NativeVulkanSceneSampledImageGeometryRange> {
    if !native_vulkan_scene_sampled_image_quad_has_recordable_geometry(quad) {
        return None;
    }
    let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
    let first_index = indices.len().min(u32::MAX as usize) as u32;
    if native_vulkan_scene_sampled_image_uses_we_effect_final_quad(quad) {
        let quad_vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
        if !native_vulkan_scene_sampled_image_vertices_visible_in_scene(&quad_vertices, scene_size)
        {
            return None;
        }
        vertices.extend_from_slice(&quad_vertices);
        indices.extend_from_slice(&[
            first_vertex,
            first_vertex.saturating_add(1),
            first_vertex.saturating_add(2),
            first_vertex.saturating_add(2),
            first_vertex.saturating_add(1),
            first_vertex.saturating_add(3),
        ]);
        return Some(NativeVulkanSceneSampledImageGeometryRange {
            first_vertex,
            vertex_count: SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT,
            first_index,
            index_count: SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT,
        });
    }
    if let Some(mesh) = &quad.mesh {
        if quad.effect_motion.is_active()
            && let Some(corners) = native_vulkan_scene_sampled_image_mesh_grid_corners(quad, mesh)
        {
            let segments =
                sampled_image_motion::grid_segments(quad.width, quad.height, quad.effect_motion);
            let before_vertices = vertices.len();
            let vertex_count = native_vulkan_scene_append_sampled_image_effect_grid_vertices(
                quad, corners, segments, vertices,
            )?;
            let grid_vertices = &vertices[before_vertices..];
            if !native_vulkan_scene_sampled_image_vertices_visible_in_scene(
                grid_vertices,
                scene_size,
            ) {
                vertices.truncate(before_vertices);
                return None;
            }
            let index_count = native_vulkan_scene_append_sampled_image_effect_grid_indices(
                first_vertex,
                segments,
                indices,
            )?;
            return Some(NativeVulkanSceneSampledImageGeometryRange {
                first_vertex,
                vertex_count,
                first_index,
                index_count,
            });
        }
        let mesh_indices = native_vulkan_scene_sampled_image_mesh_indices(mesh)?;
        let before_vertices = vertices.len();
        let before_indices = indices.len();
        let subdivided_index_count = if quad.effect_motion.is_active() {
            native_vulkan_scene_append_sampled_image_subdivided_mesh_geometry(
                quad,
                mesh,
                first_vertex,
                vertices,
                indices,
            )?
        } else {
            native_vulkan_scene_append_sampled_image_mesh_vertices(quad, mesh, vertices)?;
            0
        };
        let mesh_vertices = &vertices[before_vertices..];
        if !native_vulkan_scene_sampled_image_vertices_visible_in_scene(&mesh_vertices, scene_size)
        {
            vertices.truncate(before_vertices);
            indices.truncate(before_indices);
            return None;
        }
        let vertex_count = mesh_vertices.len().min(u32::MAX as usize) as u32;
        let index_count = if subdivided_index_count > 0 {
            subdivided_index_count
        } else {
            let index_count = mesh_indices.len().min(u32::MAX as usize) as u32;
            indices.extend(
                mesh_indices
                    .iter()
                    .map(|index| first_vertex.saturating_add(*index)),
            );
            index_count
        };
        return Some(NativeVulkanSceneSampledImageGeometryRange {
            first_vertex,
            vertex_count,
            first_index,
            index_count,
        });
    }
    if quad.effect_motion.is_active() {
        let corners = native_vulkan_scene_sampled_image_quad_grid_corners(quad)?;
        let segments =
            sampled_image_motion::grid_segments(quad.width, quad.height, quad.effect_motion);
        let before_vertices = vertices.len();
        let vertex_count = native_vulkan_scene_append_sampled_image_effect_grid_vertices(
            quad, corners, segments, vertices,
        )?;
        let grid_vertices = &vertices[before_vertices..];
        if !native_vulkan_scene_sampled_image_vertices_visible_in_scene(grid_vertices, scene_size) {
            vertices.truncate(before_vertices);
            return None;
        }
        let index_count = native_vulkan_scene_append_sampled_image_effect_grid_indices(
            first_vertex,
            segments,
            indices,
        )?;
        return Some(NativeVulkanSceneSampledImageGeometryRange {
            first_vertex,
            vertex_count,
            first_index,
            index_count,
        });
    }
    let quad_vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
    if !native_vulkan_scene_sampled_image_vertices_visible_in_scene(&quad_vertices, scene_size) {
        return None;
    }
    vertices.extend_from_slice(&quad_vertices);
    indices.extend_from_slice(&[
        first_vertex,
        first_vertex.saturating_add(1),
        first_vertex.saturating_add(2),
        first_vertex.saturating_add(2),
        first_vertex.saturating_add(1),
        first_vertex.saturating_add(3),
    ]);
    Some(NativeVulkanSceneSampledImageGeometryRange {
        first_vertex,
        vertex_count: SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT,
        first_index,
        index_count: SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT,
    })
}

fn native_vulkan_scene_append_sampled_image_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<u32> {
    if !native_vulkan_scene_sampled_image_quad_has_recordable_geometry(quad) {
        return None;
    }
    if native_vulkan_scene_sampled_image_uses_we_effect_final_quad(quad) {
        let quad_vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
        vertices.extend_from_slice(&quad_vertices);
        return Some(SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT);
    }
    if let Some(mesh) = &quad.mesh {
        let first_vertex = vertices.len();
        if quad.effect_motion.is_active()
            && let Some(corners) = native_vulkan_scene_sampled_image_mesh_grid_corners(quad, mesh)
        {
            let segments =
                sampled_image_motion::grid_segments(quad.width, quad.height, quad.effect_motion);
            return native_vulkan_scene_append_sampled_image_effect_grid_vertices(
                quad, corners, segments, vertices,
            );
        }
        if quad.effect_motion.is_active() {
            return native_vulkan_scene_append_sampled_image_subdivided_mesh_vertices(
                quad, mesh, vertices,
            );
        }
        native_vulkan_scene_append_sampled_image_mesh_vertices(quad, mesh, vertices)?;
        let vertex_count = vertices
            .len()
            .saturating_sub(first_vertex)
            .min(u32::MAX as usize) as u32;
        return Some(vertex_count);
    }
    if quad.effect_motion.is_active() {
        let corners = native_vulkan_scene_sampled_image_quad_grid_corners(quad)?;
        let segments =
            sampled_image_motion::grid_segments(quad.width, quad.height, quad.effect_motion);
        return native_vulkan_scene_append_sampled_image_effect_grid_vertices(
            quad, corners, segments, vertices,
        );
    }
    let quad_vertices = native_vulkan_scene_sampled_image_quad_vertices(quad)?;
    vertices.extend_from_slice(&quad_vertices);
    Some(SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT)
}

fn native_vulkan_scene_append_sampled_image_effect_grid_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
    corners: [NativeVulkanSceneSampledImageGridCorner; 4],
    segments: usize,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<u32> {
    if segments == 0 {
        return None;
    }
    let opacity = quad.opacity.clamp(0.0, 1.0) as f32;
    let tint = quad.tint;
    let rotation = quad.transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let vertex_count = (segments + 1).checked_mul(segments + 1)?;
    vertices.reserve(vertex_count);
    for row in 0..=segments {
        let y_factor = row as f64 / segments as f64;
        let left = native_vulkan_scene_sampled_image_grid_lerp(corners[0], corners[2], y_factor);
        let right = native_vulkan_scene_sampled_image_grid_lerp(corners[1], corners[3], y_factor);
        for column in 0..=segments {
            let x_factor = column as f64 / segments as f64;
            let point = native_vulkan_scene_sampled_image_grid_lerp(left, right, x_factor);
            let (x, y) = sampled_image_motion::apply(
                point.x,
                point.y,
                quad.width,
                quad.height,
                quad.effect_motion,
            );
            let position =
                native_vulkan_scene_transform_point_with_rotation(x, y, quad.transform, cos, sin)?;
            vertices.push(NativeVulkanSceneSampledImageVertex {
                position,
                uv: [point.u as f32, point.v as f32],
                effect_uv: native_vulkan_scene_sampled_image_vertex_effect_uv(
                    position,
                    point.x,
                    point.y,
                    [point.u as f32, point.v as f32],
                    quad,
                ),
                opacity,
                tint,
            });
        }
    }
    Some(vertex_count.min(u32::MAX as usize) as u32)
}

fn native_vulkan_scene_append_sampled_image_effect_grid_indices(
    first_vertex: u32,
    segments: usize,
    indices: &mut Vec<u32>,
) -> Option<u32> {
    if segments == 0 {
        return None;
    }
    let stride = segments + 1;
    let index_count = segments.checked_mul(segments)?.checked_mul(6)?;
    indices.reserve(index_count);
    for row in 0..segments {
        for column in 0..segments {
            let top_left = first_vertex.checked_add((row * stride + column) as u32)?;
            let top_right = top_left.checked_add(1)?;
            let bottom_left = top_left.checked_add(stride as u32)?;
            let bottom_right = bottom_left.checked_add(1)?;
            indices.extend_from_slice(&[
                top_left,
                top_right,
                bottom_left,
                bottom_left,
                top_right,
                bottom_right,
            ]);
        }
    }
    Some(index_count.min(u32::MAX as usize) as u32)
}

fn native_vulkan_scene_sampled_image_grid_lerp(
    from: NativeVulkanSceneSampledImageGridCorner,
    to: NativeVulkanSceneSampledImageGridCorner,
    factor: f64,
) -> NativeVulkanSceneSampledImageGridCorner {
    let inverse = 1.0 - factor;
    NativeVulkanSceneSampledImageGridCorner {
        x: from.x.mul_add(inverse, to.x * factor),
        y: from.y.mul_add(inverse, to.y * factor),
        u: from.u.mul_add(inverse, to.u * factor),
        v: from.v.mul_add(inverse, to.v * factor),
    }
}

fn native_vulkan_scene_sampled_image_quad_grid_corners(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<[NativeVulkanSceneSampledImageGridCorner; 4]> {
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let left = -quad.transform.anchor_x * quad.width;
    let top = -quad.transform.anchor_y * quad.height;
    let right = left + quad.width;
    let bottom = top + quad.height;
    [
        left,
        top,
        right,
        bottom,
        region.u_min,
        region.u_max,
        region.v_min,
        region.v_max,
    ]
    .into_iter()
    .all(f64::is_finite)
    .then_some([
        NativeVulkanSceneSampledImageGridCorner {
            x: left,
            y: top,
            u: region.u_min,
            v: region.v_max,
        },
        NativeVulkanSceneSampledImageGridCorner {
            x: right,
            y: top,
            u: region.u_max,
            v: region.v_max,
        },
        NativeVulkanSceneSampledImageGridCorner {
            x: left,
            y: bottom,
            u: region.u_min,
            v: region.v_min,
        },
        NativeVulkanSceneSampledImageGridCorner {
            x: right,
            y: bottom,
            u: region.u_max,
            v: region.v_min,
        },
    ])
}

fn native_vulkan_scene_sampled_image_mesh_grid_corners(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
) -> Option<[NativeVulkanSceneSampledImageGridCorner; 4]> {
    if mesh.vertices.len() != 4 || mesh.indices.as_slice() != [0, 1, 2, 2, 1, 3] {
        return None;
    }
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let u_scale = region.u_max - region.u_min;
    let v_scale = region.v_max - region.v_min;
    let local_offset_x = (0.5 - quad.transform.anchor_x) * quad.width;
    let local_offset_y = (0.5 - quad.transform.anchor_y) * quad.height;
    let mut corners = [NativeVulkanSceneSampledImageGridCorner {
        x: 0.0,
        y: 0.0,
        u: 0.0,
        v: 0.0,
    }; 4];
    for (corner, vertex) in corners.iter_mut().zip(mesh.vertices.iter()) {
        if !vertex.x.is_finite()
            || !vertex.y.is_finite()
            || !vertex.u.is_finite()
            || !vertex.v.is_finite()
        {
            return None;
        }
        *corner = NativeVulkanSceneSampledImageGridCorner {
            x: vertex.x + local_offset_x,
            y: vertex.y + local_offset_y,
            u: region.u_min + vertex.u * u_scale,
            v: region.v_min + vertex.v * v_scale,
        };
    }
    Some(corners)
}

#[inline]
fn native_vulkan_scene_sampled_image_quad_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<[NativeVulkanSceneSampledImageVertex; 4]> {
    let points = native_vulkan_scene_sampled_image_quad_positions(quad)?;
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let uvs = [
        [region.u_min as f32, region.v_max as f32],
        [region.u_max as f32, region.v_max as f32],
        [region.u_min as f32, region.v_min as f32],
        [region.u_max as f32, region.v_min as f32],
    ];
    let mut vertices = [NativeVulkanSceneSampledImageVertex {
        position: [0.0, 0.0],
        uv: [0.0, 0.0],
        effect_uv: [0.0, 0.0],
        opacity: quad.opacity.clamp(0.0, 1.0) as f32,
        tint: quad.tint,
    }; 4];
    let effect_uvs = [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
    for (((vertex, position), uv), effect_uv) in
        vertices.iter_mut().zip(points).zip(uvs).zip(effect_uvs)
    {
        vertex.position = position;
        vertex.uv = uv;
        vertex.effect_uv = if quad.effect_uv_space.is_some() {
            native_vulkan_scene_sampled_image_vertex_effect_uv(position, 0.0, 0.0, uv, quad)
        } else {
            effect_uv
        };
    }
    Some(vertices)
}

fn native_vulkan_scene_sampled_image_quad_positions(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<[[f32; 2]; 4]> {
    if !quad.effect_motion.is_active() {
        return native_vulkan_scene_quad_positions(quad.width, quad.height, quad.transform);
    }
    let left = -quad.transform.anchor_x * quad.width;
    let top = -quad.transform.anchor_y * quad.height;
    let right = left + quad.width;
    let bottom = top + quad.height;
    let rotation = quad.transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let points = [(left, top), (right, top), (left, bottom), (right, bottom)];
    let mut positions = [[0.0, 0.0]; 4];
    for (position, (x, y)) in positions.iter_mut().zip(points) {
        let (x, y) = sampled_image_motion::apply(x, y, quad.width, quad.height, quad.effect_motion);
        *position =
            native_vulkan_scene_transform_point_with_rotation(x, y, quad.transform, cos, sin)?;
    }
    Some(positions)
}

fn native_vulkan_scene_sampled_image_mesh_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
) -> Option<(Vec<NativeVulkanSceneSampledImageVertex>, Vec<u32>)> {
    if mesh.vertices.len() < 3
        || mesh.indices.len() < 3
        || mesh.indices.len() % 3 != 0
        || !quad.width.is_finite()
        || quad.width <= 0.0
        || !quad.height.is_finite()
        || quad.height <= 0.0
    {
        return None;
    }
    let mesh_indices = native_vulkan_scene_sampled_image_mesh_indices(mesh)?;
    let mut vertices = Vec::with_capacity(mesh.vertices.len());
    native_vulkan_scene_append_sampled_image_mesh_vertices(quad, mesh, &mut vertices)?;
    Some((vertices, mesh_indices.to_vec()))
}

fn native_vulkan_scene_append_sampled_image_effect_base_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<NativeVulkanSceneSampledImageGeometryRange> {
    let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
    let first_index = indices.len().min(u32::MAX as usize) as u32;
    let index_count = if let Some(mesh) = &quad.mesh {
        native_vulkan_scene_append_sampled_image_effect_base_mesh_geometry(
            quad,
            mesh,
            first_vertex,
            vertices,
            indices,
        )?
    } else {
        native_vulkan_scene_append_sampled_image_effect_base_quad_geometry(
            quad,
            first_vertex,
            vertices,
            indices,
        )?
    };
    Some(NativeVulkanSceneSampledImageGeometryRange {
        first_vertex,
        vertex_count: (vertices.len() as u32).saturating_sub(first_vertex),
        first_index,
        index_count,
    })
}

fn native_vulkan_scene_append_sampled_image_effect_final_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    scene_size: Option<SceneSize>,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<NativeVulkanSceneSampledImageGeometryRange> {
    let mut final_quad = quad.clone();
    final_quad.mesh = None;
    final_quad.effect_motion = SceneNativeEffectMotion::default();
    final_quad.effect_uv_space = native_vulkan_scene_sampled_image_effect_final_uv_space(quad);
    native_vulkan_scene_append_sampled_image_geometry(&final_quad, scene_size, vertices, indices)
}

fn native_vulkan_scene_sampled_image_effect_final_uv_space(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<NativeVulkanSceneEffectUvSpace> {
    let effect_pass = quad.effect_target_pass.as_ref()?;
    effect_pass.alpha_texture_slot?;
    Some(native_vulkan_scene_effect_uv_space_from_transform(
        effect_pass.effect_uv_transform,
        quad.width,
        quad.height,
        quad.texture_region,
        quad.transform,
    ))
}

fn native_vulkan_scene_append_sampled_image_effect_base_quad_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    first_vertex: u32,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<u32> {
    if !quad.width.is_finite()
        || quad.width <= 0.0
        || !quad.height.is_finite()
        || quad.height <= 0.0
    {
        return None;
    }
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let opacity = quad.opacity.clamp(0.0, 1.0) as f32;
    let target_vertices = [
        (
            [0.0, 0.0],
            [region.u_min as f32, region.v_max as f32],
            [0.0, 1.0],
        ),
        (
            [quad.width as f32, 0.0],
            [region.u_max as f32, region.v_max as f32],
            [1.0, 1.0],
        ),
        (
            [0.0, quad.height as f32],
            [region.u_min as f32, region.v_min as f32],
            [0.0, 0.0],
        ),
        (
            [quad.width as f32, quad.height as f32],
            [region.u_max as f32, region.v_min as f32],
            [1.0, 0.0],
        ),
    ];
    for (position, uv, effect_uv) in target_vertices {
        vertices.push(NativeVulkanSceneSampledImageVertex {
            position,
            uv,
            effect_uv,
            opacity,
            tint: quad.tint,
        });
    }
    indices.extend_from_slice(&[
        first_vertex,
        first_vertex + 1,
        first_vertex + 2,
        first_vertex + 2,
        first_vertex + 1,
        first_vertex + 3,
    ]);
    Some(SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT)
}

fn native_vulkan_scene_append_sampled_image_effect_base_mesh_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
    first_vertex: u32,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<u32> {
    if mesh.vertices.len() < 3
        || mesh.indices.len() < 3
        || mesh.indices.len() % 3 != 0
        || !quad.width.is_finite()
        || quad.width <= 0.0
        || !quad.height.is_finite()
        || quad.height <= 0.0
    {
        return None;
    }
    let mesh_indices = native_vulkan_scene_sampled_image_mesh_indices(mesh)?;
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let opacity = quad.opacity.clamp(0.0, 1.0) as f32;
    vertices.reserve(mesh.vertices.len());
    for vertex in &mesh.vertices {
        if !vertex.x.is_finite()
            || !vertex.y.is_finite()
            || !vertex.u.is_finite()
            || !vertex.v.is_finite()
        {
            return None;
        }
        let x = vertex.x + quad.width * 0.5;
        let y = quad.height * 0.5 - vertex.y;
        let uv = native_vulkan_scene_sampled_image_effect_base_mesh_uv(vertex, region);
        vertices.push(NativeVulkanSceneSampledImageVertex {
            position: [x as f32, y as f32],
            uv,
            effect_uv: native_vulkan_scene_sampled_image_effect_uv(
                vertex.x,
                vertex.y,
                quad.width,
                quad.height,
                SceneTransform {
                    anchor_x: 0.5,
                    anchor_y: 0.5,
                    ..SceneTransform::default()
                },
            ),
            opacity: native_vulkan_scene_sampled_image_vertex_opacity(opacity, vertex.opacity),
            tint: quad.tint,
        });
    }
    let index_count = mesh_indices.len().min(u32::MAX as usize) as u32;
    indices.extend(
        mesh_indices
            .iter()
            .map(|index| first_vertex.saturating_add(*index)),
    );
    Some(index_count)
}

fn native_vulkan_scene_sampled_image_effect_base_mesh_uv(
    vertex: &SceneMeshVertex,
    region: SceneTextureRegion,
) -> [f32; 2] {
    let u_scale = region.u_max - region.u_min;
    let v_scale = region.v_max - region.v_min;
    [
        (region.u_min + vertex.u * u_scale) as f32,
        (region.v_min + (1.0 - vertex.v) * v_scale) as f32,
    ]
}

fn native_vulkan_scene_sampled_image_subdivided_mesh_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
) -> Option<(Vec<NativeVulkanSceneSampledImageVertex>, Vec<u32>)> {
    native_vulkan_scene_sampled_image_mesh_indices(mesh)?;
    let subdivision =
        sampled_image_motion::mesh_subdivision(quad.width, quad.height, quad.effect_motion, mesh)?;
    if subdivision <= 1 {
        return native_vulkan_scene_sampled_image_mesh_geometry(quad, mesh);
    }
    let triangle_count = mesh.indices.len() / 3;
    let vertices_per_triangle =
        sampled_image_motion::subdivided_triangle_vertex_count(subdivision)?;
    let mut vertices = Vec::with_capacity(triangle_count.saturating_mul(vertices_per_triangle));
    let mut indices =
        Vec::with_capacity(triangle_count.saturating_mul(subdivision * subdivision * 3));
    native_vulkan_scene_append_sampled_image_subdivided_mesh_geometry(
        quad,
        mesh,
        0,
        &mut vertices,
        &mut indices,
    )?;
    Some((vertices, indices))
}

fn native_vulkan_scene_append_sampled_image_subdivided_mesh_geometry(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
    first_vertex: u32,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
    indices: &mut Vec<u32>,
) -> Option<u32> {
    let subdivision =
        sampled_image_motion::mesh_subdivision(quad.width, quad.height, quad.effect_motion, mesh)?;
    if subdivision <= 1 {
        native_vulkan_scene_append_sampled_image_mesh_vertices(quad, mesh, vertices)?;
        let mesh_indices = native_vulkan_scene_sampled_image_mesh_indices(mesh)?;
        let index_count = mesh_indices.len().min(u32::MAX as usize) as u32;
        indices.extend(
            mesh_indices
                .iter()
                .map(|index| first_vertex.saturating_add(*index)),
        );
        return Some(index_count);
    }
    let first_generated_vertex = vertices.len().min(u32::MAX as usize) as u32;
    native_vulkan_scene_append_sampled_image_subdivided_mesh_vertices_with_subdivision(
        quad,
        mesh,
        subdivision,
        vertices,
    )?;
    sampled_image_motion::append_subdivided_mesh_indices(
        first_generated_vertex,
        mesh.indices.len() / 3,
        subdivision,
        indices,
    )
}

fn native_vulkan_scene_append_sampled_image_subdivided_mesh_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<u32> {
    let subdivision =
        sampled_image_motion::mesh_subdivision(quad.width, quad.height, quad.effect_motion, mesh)?;
    if subdivision <= 1 {
        native_vulkan_scene_append_sampled_image_mesh_vertices(quad, mesh, vertices)?;
        return Some(mesh.vertices.len().min(u32::MAX as usize) as u32);
    }
    let before_vertices = vertices.len();
    native_vulkan_scene_append_sampled_image_subdivided_mesh_vertices_with_subdivision(
        quad,
        mesh,
        subdivision,
        vertices,
    )?;
    Some(
        vertices
            .len()
            .saturating_sub(before_vertices)
            .min(u32::MAX as usize) as u32,
    )
}

fn native_vulkan_scene_append_sampled_image_subdivided_mesh_vertices_with_subdivision(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
    subdivision: usize,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<()> {
    if subdivision == 0
        || mesh.vertices.len() < 3
        || mesh.indices.len() < 3
        || mesh.indices.len() % 3 != 0
        || !quad.width.is_finite()
        || quad.width <= 0.0
        || !quad.height.is_finite()
        || quad.height <= 0.0
    {
        return None;
    }
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let u_scale = region.u_max - region.u_min;
    let v_scale = region.v_max - region.v_min;
    let opacity = quad.opacity.clamp(0.0, 1.0) as f32;
    let tint = quad.tint;
    let local_offset_x = (0.5 - quad.transform.anchor_x) * quad.width;
    let local_offset_y = (0.5 - quad.transform.anchor_y) * quad.height;
    let rotation = quad.transform.rotation_deg.to_radians();
    let (sin, cos) = rotation.sin_cos();
    let vertices_per_triangle =
        sampled_image_motion::subdivided_triangle_vertex_count(subdivision)?;
    vertices.reserve((mesh.indices.len() / 3).saturating_mul(vertices_per_triangle));
    for triangle in mesh.indices.chunks_exact(3) {
        let a = native_vulkan_scene_sampled_image_mesh_point(
            mesh,
            triangle[0],
            local_offset_x,
            local_offset_y,
            region.u_min,
            region.v_min,
            u_scale,
            v_scale,
        )?;
        let b = native_vulkan_scene_sampled_image_mesh_point(
            mesh,
            triangle[1],
            local_offset_x,
            local_offset_y,
            region.u_min,
            region.v_min,
            u_scale,
            v_scale,
        )?;
        let c = native_vulkan_scene_sampled_image_mesh_point(
            mesh,
            triangle[2],
            local_offset_x,
            local_offset_y,
            region.u_min,
            region.v_min,
            u_scale,
            v_scale,
        )?;
        for row in 0..=subdivision {
            for column in 0..=subdivision - row {
                let b_weight = row as f64 / subdivision as f64;
                let c_weight = column as f64 / subdivision as f64;
                let a_weight = 1.0 - b_weight - c_weight;
                let point = NativeVulkanSceneSampledImageMeshPoint {
                    x: a.x.mul_add(a_weight, b.x.mul_add(b_weight, c.x * c_weight)),
                    y: a.y.mul_add(a_weight, b.y.mul_add(b_weight, c.y * c_weight)),
                    u: a.u.mul_add(a_weight, b.u.mul_add(b_weight, c.u * c_weight)),
                    v: a.v.mul_add(a_weight, b.v.mul_add(b_weight, c.v * c_weight)),
                    opacity: a
                        .opacity
                        .mul_add(a_weight, b.opacity.mul_add(b_weight, c.opacity * c_weight)),
                };
                native_vulkan_scene_push_sampled_image_mesh_vertex(
                    quad, point, opacity, tint, cos, sin, vertices,
                )?;
            }
        }
    }
    Some(())
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_scene_sampled_image_mesh_point(
    mesh: &SceneMesh,
    index: u32,
    local_offset_x: f64,
    local_offset_y: f64,
    u_min: f64,
    v_min: f64,
    u_scale: f64,
    v_scale: f64,
) -> Option<NativeVulkanSceneSampledImageMeshPoint> {
    let vertex = mesh.vertices.get(usize::try_from(index).ok()?)?;
    if !vertex.x.is_finite()
        || !vertex.y.is_finite()
        || !vertex.u.is_finite()
        || !vertex.v.is_finite()
    {
        return None;
    }
    Some(NativeVulkanSceneSampledImageMeshPoint {
        x: vertex.x + local_offset_x,
        y: vertex.y + local_offset_y,
        u: u_min + vertex.u * u_scale,
        v: v_min + vertex.v * v_scale,
        opacity: vertex.opacity,
    })
}

fn native_vulkan_scene_push_sampled_image_mesh_vertex(
    quad: &NativeVulkanSceneSampledImageQuad,
    point: NativeVulkanSceneSampledImageMeshPoint,
    opacity: f32,
    tint: [f32; 4],
    cos: f64,
    sin: f64,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<()> {
    let (x, y) = sampled_image_motion::apply(
        point.x,
        point.y,
        quad.width,
        quad.height,
        quad.effect_motion,
    );
    let position =
        native_vulkan_scene_transform_point_with_rotation(x, y, quad.transform, cos, sin)?;
    vertices.push(NativeVulkanSceneSampledImageVertex {
        position,
        uv: [point.u as f32, point.v as f32],
        effect_uv: native_vulkan_scene_sampled_image_vertex_effect_uv(
            position,
            point.x,
            point.y,
            [point.u as f32, point.v as f32],
            quad,
        ),
        opacity: native_vulkan_scene_sampled_image_vertex_opacity(opacity, point.opacity),
        tint,
    });
    Some(())
}

fn native_vulkan_scene_sampled_image_vertex_opacity(
    layer_opacity: f32,
    vertex_opacity: f64,
) -> f32 {
    layer_opacity * vertex_opacity.clamp(0.0, 1.0) as f32
}

fn native_vulkan_scene_sampled_image_effect_uv(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    transform: SceneTransform,
) -> [f32; 2] {
    if width <= f64::EPSILON || height <= f64::EPSILON {
        return [0.0, 0.0];
    }
    let u = ((x + transform.anchor_x * width) / width).clamp(0.0, 1.0);
    let v = (1.0 - ((y + transform.anchor_y * height) / height)).clamp(0.0, 1.0);
    [u as f32, v as f32]
}

fn native_vulkan_scene_sampled_image_vertex_effect_uv(
    scene_position: [f32; 2],
    local_x: f64,
    local_y: f64,
    base_uv: [f32; 2],
    quad: &NativeVulkanSceneSampledImageQuad,
) -> [f32; 2] {
    if let Some(space) = quad.effect_uv_space {
        return match space.mapping {
            NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
                scale_u,
                scale_v,
                offset_u,
                offset_v,
            } => [
                (f64::from(base_uv[0]).mul_add(scale_u, offset_u)) as f32,
                (f64::from(base_uv[1]).mul_add(scale_v, offset_v)) as f32,
            ],
            NativeVulkanSceneEffectUvMapping::ScenePositionBounds => {
                native_vulkan_scene_sampled_image_effect_uv_from_scene_position(
                    scene_position,
                    space,
                )
            }
        };
    }
    native_vulkan_scene_sampled_image_effect_uv(
        local_x,
        local_y,
        quad.width,
        quad.height,
        quad.transform,
    )
}

fn native_vulkan_scene_sampled_image_effect_uv_from_scene_position(
    scene_position: [f32; 2],
    space: NativeVulkanSceneEffectUvSpace,
) -> [f32; 2] {
    if space.width <= f64::EPSILON
        || space.height <= f64::EPSILON
        || space.transform.scale_x.abs() <= f64::EPSILON
        || space.transform.scale_y.abs() <= f64::EPSILON
    {
        return [0.0, 0.0];
    }
    let dx = f64::from(scene_position[0]) - space.transform.x;
    let dy = f64::from(scene_position[1]) - space.transform.y;
    let rotation = space.transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let local_x = (dx.mul_add(cos, dy * sin)) / space.transform.scale_x;
    let local_y = ((-dx).mul_add(sin, dy * cos)) / space.transform.scale_y;
    let mut uv = if let Some(bounds) = space.bounds {
        native_vulkan_scene_sampled_image_effect_uv_from_bounds(local_x, local_y, bounds)
    } else {
        native_vulkan_scene_sampled_image_effect_uv(
            local_x,
            local_y,
            space.width,
            space.height,
            space.transform,
        )
    };
    if let Some(region) = space.texture_region {
        uv[0] = (region.u_min + (region.u_max - region.u_min) * f64::from(uv[0])) as f32;
        uv[1] = (region.v_min + (region.v_max - region.v_min) * f64::from(uv[1])) as f32;
    }
    uv
}

fn native_vulkan_scene_sampled_image_effect_uv_from_bounds(
    local_x: f64,
    local_y: f64,
    bounds: NativeVulkanSceneEffectUvBounds,
) -> [f32; 2] {
    if bounds.width <= f64::EPSILON || bounds.height <= f64::EPSILON {
        return [0.0, 0.0];
    }
    let u = ((local_x - bounds.left) / bounds.width).clamp(0.0, 1.0);
    let v = (1.0 - ((local_y - bounds.top) / bounds.height)).clamp(0.0, 1.0);
    [u as f32, v as f32]
}

fn native_vulkan_scene_append_sampled_image_mesh_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
    vertices: &mut Vec<NativeVulkanSceneSampledImageVertex>,
) -> Option<()> {
    if mesh.vertices.len() < 3
        || mesh.indices.len() < 3
        || mesh.indices.len() % 3 != 0
        || !quad.width.is_finite()
        || quad.width <= 0.0
        || !quad.height.is_finite()
        || quad.height <= 0.0
    {
        return None;
    }
    let region = quad.texture_region.unwrap_or(SceneTextureRegion {
        u_min: 0.0,
        v_min: 0.0,
        u_max: 1.0,
        v_max: 1.0,
        frame_index: 0,
        frame_count: 1,
        columns: 1,
        rows: 1,
        fps: None,
        loop_playback: true,
    });
    let u_scale = region.u_max - region.u_min;
    let v_scale = region.v_max - region.v_min;
    let opacity = quad.opacity.clamp(0.0, 1.0) as f32;
    let tint = quad.tint;
    let local_offset_x = (0.5 - quad.transform.anchor_x) * quad.width;
    let local_offset_y = (0.5 - quad.transform.anchor_y) * quad.height;
    let rotation = quad.transform.rotation_deg.to_radians();
    let (sin, cos) = rotation.sin_cos();
    vertices.reserve(mesh.vertices.len());
    for vertex in &mesh.vertices {
        if !vertex.x.is_finite()
            || !vertex.y.is_finite()
            || !vertex.u.is_finite()
            || !vertex.v.is_finite()
        {
            return None;
        }
        let x = vertex.x + local_offset_x;
        let y = vertex.y + local_offset_y;
        let effect_local_x = x;
        let effect_local_y = y;
        let (x, y) = sampled_image_motion::apply(x, y, quad.width, quad.height, quad.effect_motion);
        let position =
            native_vulkan_scene_transform_point_with_rotation(x, y, quad.transform, cos, sin)?;
        vertices.push(NativeVulkanSceneSampledImageVertex {
            position,
            uv: [
                (region.u_min + vertex.u * u_scale) as f32,
                (region.v_min + vertex.v * v_scale) as f32,
            ],
            effect_uv: native_vulkan_scene_sampled_image_vertex_effect_uv(
                position,
                effect_local_x,
                effect_local_y,
                [
                    (region.u_min + vertex.u * u_scale) as f32,
                    (region.v_min + vertex.v * v_scale) as f32,
                ],
                quad,
            ),
            opacity: native_vulkan_scene_sampled_image_vertex_opacity(opacity, vertex.opacity),
            tint,
        });
    }
    Some(())
}

fn native_vulkan_scene_sampled_image_mesh_indices(mesh: &SceneMesh) -> Option<&[u32]> {
    if mesh
        .indices
        .iter()
        .any(|index| usize::try_from(*index).map_or(true, |index| index >= mesh.vertices.len()))
    {
        return None;
    }
    Some(&mesh.indices)
}

fn native_vulkan_scene_video_vertices(
    quad: &NativeVulkanSceneVideoQuad,
) -> Option<[NativeVulkanSceneSampledImageVertex; 4]> {
    let points = native_vulkan_scene_quad_positions(quad.width, quad.height, quad.transform)?;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    let mut vertices = [NativeVulkanSceneSampledImageVertex {
        position: [0.0, 0.0],
        uv: [0.0, 0.0],
        effect_uv: [0.0, 0.0],
        opacity: quad.opacity.clamp(0.0, 1.0) as f32,
        tint: SCENE_SAMPLED_IMAGE_DEFAULT_TINT,
    }; 4];
    for ((vertex, position), uv) in vertices.iter_mut().zip(points).zip(uvs) {
        vertex.position = position;
        vertex.uv = uv;
        vertex.effect_uv = uv;
    }
    Some(vertices)
}

fn native_vulkan_scene_recordable_quad(
    op: &NativeVulkanSceneDrawOp,
) -> Option<NativeVulkanSceneRecordableQuad> {
    match op.kind {
        NativeVulkanSceneDrawOpKind::ColorQuad => {
            native_vulkan_scene_recordable_quad_from_op(op, "color-quad")
        }
        NativeVulkanSceneDrawOpKind::Rectangle => native_vulkan_scene_recordable_quad_from_op(
            op,
            native_vulkan_scene_rectangle_recordable_kind(op),
        ),
        NativeVulkanSceneDrawOpKind::Ellipse => {
            native_vulkan_scene_recordable_quad_from_op(op, "ellipse")
        }
        NativeVulkanSceneDrawOpKind::Path => {
            native_vulkan_scene_recordable_quad_from_op(op, "path")
        }
        NativeVulkanSceneDrawOpKind::Text => {
            native_vulkan_scene_recordable_quad_from_op(op, "text")
        }
        NativeVulkanSceneDrawOpKind::AudioResponse => {
            native_vulkan_scene_recordable_quad_from_op(op, "audio-response")
        }
        _ => None,
    }
}

fn native_vulkan_scene_sampled_image_quad(
    op: &NativeVulkanSceneDrawOp,
) -> Option<NativeVulkanSceneSampledImageQuad> {
    if op.kind != NativeVulkanSceneDrawOpKind::Image
        || native_vulkan_scene_full_extent_sampled_image_op_ready(op)
    {
        return None;
    }
    let texture_slots = native_vulkan_scene_texture_slots_from_render_slots(&op.texture_slots);
    let effect_passes =
        native_vulkan_scene_effect_passes_from_render_passes(&op.image_effect_passes);
    let material_pass = native_vulkan_scene_sampled_image_material_pass(
        NativeVulkanSceneMaterialKind::SampledImage,
        op.blend_mode,
        op.alpha_texture_slot,
        op.alpha_texture_mode,
        texture_slots.len(),
        &effect_passes,
    );
    Some(NativeVulkanSceneSampledImageQuad {
        layer_index: op.layer_index,
        layer_id: op.layer_id.clone(),
        source: op.source.clone()?,
        texture_slots,
        image_effect_pass_count: effect_passes.len(),
        effect_target_pass: native_vulkan_scene_render_first_class_effect_target_pass(
            &op.image_effect_passes,
        ),
        material_pass,
        base_blend_mode: op.blend_mode,
        effect_passes,
        composite_key: op.composite_key.clone(),
        fit: op.fit,
        opacity: op.opacity,
        tint: native_vulkan_scene_tint_from_color(op.color.as_deref()),
        width: op.width.unwrap_or(0.0),
        height: op.height.unwrap_or(0.0),
        mesh: op.mesh.clone(),
        effect_uv_space: op.effect_uv_space.clone(),
        effect_motion: op.effect_motion,
        texture_region: op.texture_region,
        transform: op.transform,
    })
}

fn native_vulkan_scene_video_quad(
    op: &NativeVulkanSceneDrawOp,
) -> Option<NativeVulkanSceneVideoQuad> {
    if op.kind != NativeVulkanSceneDrawOpKind::Video {
        return None;
    }
    Some(NativeVulkanSceneVideoQuad {
        layer_index: op.layer_index,
        layer_id: op.layer_id.clone(),
        source: op.source.clone()?,
        fit: op.fit,
        opacity: op.opacity,
        width: op.width?,
        height: op.height?,
        transform: op.transform,
    })
}

fn native_vulkan_scene_recordable_quad_from_op(
    op: &NativeVulkanSceneDrawOp,
    kind: &'static str,
) -> Option<NativeVulkanSceneRecordableQuad> {
    let fill_color = op
        .color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned);
    let fill_rgba = fill_color
        .as_deref()
        .and_then(|color| native_vulkan_scene_rgba_from_hex(color, op.opacity));
    let stroke_color = op
        .stroke_color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned);
    let stroke_rgba = stroke_color
        .as_deref()
        .and_then(|color| native_vulkan_scene_rgba_from_hex(color, op.opacity));
    let stroke_width = stroke_rgba.map(|_| op.stroke_width.unwrap_or(1.0));
    let (color, rgba) = fill_color
        .clone()
        .zip(fill_rgba)
        .or_else(|| stroke_color.clone().zip(stroke_rgba))?;
    Some(NativeVulkanSceneRecordableQuad {
        layer_index: op.layer_index,
        layer_id: op.layer_id.clone(),
        kind,
        color,
        rgba,
        blend: native_vulkan_scene_blend_state(op.blend_mode),
        fill_color,
        fill_rgba,
        stroke_color,
        stroke_rgba,
        stroke_width,
        width: op.width,
        height: op.height,
        corner_radius: op.corner_radius,
        text: op.text.clone(),
        font_size: op.font_size,
        font_family: op.font_family.clone(),
        font_source: op.font_source.clone(),
        font_weight: op.font_weight.clone(),
        text_align: op.text_align,
        path_data: op.path_data.clone(),
        path_fill_rule: op.path_fill_rule,
        transform: op.transform,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeVulkanScenePathToken {
    Command(char),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq)]
struct NativeVulkanScenePathSubpath {
    points: Vec<[f64; 2]>,
    closed: bool,
}

fn native_vulkan_scene_simple_path_points(path: &str) -> Option<Vec<[f64; 2]>> {
    let subpaths = native_vulkan_scene_path_subpaths(path)?;
    if subpaths.len() == 1 {
        subpaths.into_iter().next().map(|subpath| subpath.points)
    } else {
        None
    }
}

fn native_vulkan_scene_path_subpaths(path: &str) -> Option<Vec<NativeVulkanScenePathSubpath>> {
    let tokens = native_vulkan_scene_path_tokens(path)?;
    let mut index = 0usize;
    let mut command = None::<char>;
    let mut subpaths = Vec::new();
    let mut current_points = Vec::new();
    let mut current = [0.0, 0.0];
    let mut start = [0.0, 0.0];
    let mut previous_cubic_control = None::<[f64; 2]>;
    let mut previous_quadratic_control = None::<[f64; 2]>;

    while index < tokens.len() {
        if let NativeVulkanScenePathToken::Command(value) = tokens[index] {
            command = Some(value);
            index += 1;
        }
        let command = command?;
        match command {
            'M' | 'm' => {
                let relative = command == 'm';
                let mut first = true;
                while let Some((x, y, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    index = next_index;
                    let point = if relative {
                        [current[0] + x, current[1] + y]
                    } else {
                        [x, y]
                    };
                    current = point;
                    if first {
                        native_vulkan_scene_finish_path_subpath(
                            &mut subpaths,
                            &mut current_points,
                            false,
                        );
                        start = point;
                        first = false;
                    }
                    current_points.push(point);
                    previous_cubic_control = None;
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'L' | 'l' => {
                let relative = command == 'l';
                while let Some((x, y, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    index = next_index;
                    current = if relative {
                        [current[0] + x, current[1] + y]
                    } else {
                        [x, y]
                    };
                    current_points.push(current);
                    previous_cubic_control = None;
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'H' | 'h' => {
                let relative = command == 'h';
                while let Some((x, next_index)) =
                    native_vulkan_scene_take_path_number(&tokens, index)
                {
                    index = next_index;
                    current[0] = if relative { current[0] + x } else { x };
                    current_points.push(current);
                    previous_cubic_control = None;
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'V' | 'v' => {
                let relative = command == 'v';
                while let Some((y, next_index)) =
                    native_vulkan_scene_take_path_number(&tokens, index)
                {
                    index = next_index;
                    current[1] = if relative { current[1] + y } else { y };
                    current_points.push(current);
                    previous_cubic_control = None;
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'C' | 'c' => {
                let relative = command == 'c';
                while let Some((x1, y1, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    let (x2, y2, next_index) =
                        native_vulkan_scene_take_path_pair(&tokens, next_index)?;
                    let (x, y, next_index) =
                        native_vulkan_scene_take_path_pair(&tokens, next_index)?;
                    index = next_index;
                    let control_1 = native_vulkan_scene_path_point(current, x1, y1, relative);
                    let control_2 = native_vulkan_scene_path_point(current, x2, y2, relative);
                    let end = native_vulkan_scene_path_point(current, x, y, relative);
                    native_vulkan_scene_push_cubic_curve_points(
                        &mut current_points,
                        current,
                        control_1,
                        control_2,
                        end,
                    )?;
                    current = end;
                    previous_cubic_control = Some(control_2);
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'S' | 's' => {
                let relative = command == 's';
                while let Some((x2, y2, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    let (x, y, next_index) =
                        native_vulkan_scene_take_path_pair(&tokens, next_index)?;
                    index = next_index;
                    let control_1 = native_vulkan_scene_reflected_control_point(
                        current,
                        previous_cubic_control,
                    );
                    let control_2 = native_vulkan_scene_path_point(current, x2, y2, relative);
                    let end = native_vulkan_scene_path_point(current, x, y, relative);
                    native_vulkan_scene_push_cubic_curve_points(
                        &mut current_points,
                        current,
                        control_1,
                        control_2,
                        end,
                    )?;
                    current = end;
                    previous_cubic_control = Some(control_2);
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'Q' | 'q' => {
                let relative = command == 'q';
                while let Some((x1, y1, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    let (x, y, next_index) =
                        native_vulkan_scene_take_path_pair(&tokens, next_index)?;
                    index = next_index;
                    let control = native_vulkan_scene_path_point(current, x1, y1, relative);
                    let end = native_vulkan_scene_path_point(current, x, y, relative);
                    native_vulkan_scene_push_quadratic_curve_points(
                        &mut current_points,
                        current,
                        control,
                        end,
                    )?;
                    current = end;
                    previous_cubic_control = None;
                    previous_quadratic_control = Some(control);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'T' | 't' => {
                let relative = command == 't';
                while let Some((x, y, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    index = next_index;
                    let control = native_vulkan_scene_reflected_control_point(
                        current,
                        previous_quadratic_control,
                    );
                    let end = native_vulkan_scene_path_point(current, x, y, relative);
                    native_vulkan_scene_push_quadratic_curve_points(
                        &mut current_points,
                        current,
                        control,
                        end,
                    )?;
                    current = end;
                    previous_cubic_control = None;
                    previous_quadratic_control = Some(control);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'A' | 'a' => {
                let relative = command == 'a';
                while let Some((rx, ry, next_index)) =
                    native_vulkan_scene_take_path_pair(&tokens, index)
                {
                    let (x_axis_rotation, next_index) =
                        native_vulkan_scene_take_path_number(&tokens, next_index)?;
                    let (large_arc_flag, next_index) =
                        native_vulkan_scene_take_path_number(&tokens, next_index)?;
                    let (sweep_flag, next_index) =
                        native_vulkan_scene_take_path_number(&tokens, next_index)?;
                    let (x, y, next_index) =
                        native_vulkan_scene_take_path_pair(&tokens, next_index)?;
                    index = next_index;
                    let end = native_vulkan_scene_path_point(current, x, y, relative);
                    native_vulkan_scene_push_arc_points(
                        &mut current_points,
                        current,
                        rx,
                        ry,
                        x_axis_rotation,
                        native_vulkan_scene_path_arc_flag(large_arc_flag)?,
                        native_vulkan_scene_path_arc_flag(sweep_flag)?,
                        end,
                    )?;
                    current = end;
                    previous_cubic_control = None;
                    previous_quadratic_control = None;
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'Z' | 'z' => {
                current = start;
                if current_points
                    .last()
                    .is_some_and(|point| native_vulkan_scene_path_points_close(*point, start))
                {
                    let _ = current_points.pop();
                }
                native_vulkan_scene_finish_path_subpath(&mut subpaths, &mut current_points, true);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            _ => return None,
        }
    }

    native_vulkan_scene_finish_path_subpath(&mut subpaths, &mut current_points, false);
    Some(subpaths)
}

fn native_vulkan_scene_finish_path_subpath(
    subpaths: &mut Vec<NativeVulkanScenePathSubpath>,
    points: &mut Vec<[f64; 2]>,
    closed: bool,
) {
    points.dedup_by(|left, right| native_vulkan_scene_path_points_close(*left, *right));
    if points.len() >= 2 {
        subpaths.push(NativeVulkanScenePathSubpath {
            points: std::mem::take(points),
            closed,
        });
    } else {
        points.clear();
    }
}

fn native_vulkan_scene_path_points_close(left: [f64; 2], right: [f64; 2]) -> bool {
    (left[0] - right[0]).abs() <= SCENE_FULL_PATH_POINT_EPSILON
        && (left[1] - right[1]).abs() <= SCENE_FULL_PATH_POINT_EPSILON
}

fn native_vulkan_scene_path_tokens(path: &str) -> Option<Vec<NativeVulkanScenePathToken>> {
    let mut tokens = Vec::new();
    let chars = path.as_bytes();
    let mut index = 0usize;
    while index < chars.len() {
        let byte = chars[index];
        if byte.is_ascii_whitespace() || byte == b',' {
            index += 1;
            continue;
        }
        let ch = byte as char;
        if matches!(
            ch,
            'M' | 'm'
                | 'L'
                | 'l'
                | 'H'
                | 'h'
                | 'V'
                | 'v'
                | 'C'
                | 'c'
                | 'S'
                | 's'
                | 'Q'
                | 'q'
                | 'T'
                | 't'
                | 'A'
                | 'a'
                | 'Z'
                | 'z'
        ) {
            tokens.push(NativeVulkanScenePathToken::Command(ch));
            index += 1;
            continue;
        }
        if byte == b'+' || byte == b'-' || byte == b'.' || byte.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len() {
                let next = chars[index];
                if next.is_ascii_digit()
                    || next == b'.'
                    || next == b'e'
                    || next == b'E'
                    || ((next == b'+' || next == b'-')
                        && matches!(chars.get(index.wrapping_sub(1)), Some(b'e' | b'E')))
                {
                    index += 1;
                } else {
                    break;
                }
            }
            let value = path[start..index].parse::<f64>().ok()?;
            if !value.is_finite() {
                return None;
            }
            tokens.push(NativeVulkanScenePathToken::Number(value));
            continue;
        }
        return None;
    }
    Some(tokens)
}

fn native_vulkan_scene_take_path_number(
    tokens: &[NativeVulkanScenePathToken],
    index: usize,
) -> Option<(f64, usize)> {
    match tokens.get(index)? {
        NativeVulkanScenePathToken::Number(value) => Some((*value, index + 1)),
        NativeVulkanScenePathToken::Command(_) => None,
    }
}

fn native_vulkan_scene_take_path_pair(
    tokens: &[NativeVulkanScenePathToken],
    index: usize,
) -> Option<(f64, f64, usize)> {
    let (x, index) = native_vulkan_scene_take_path_number(tokens, index)?;
    let (y, index) = native_vulkan_scene_take_path_number(tokens, index)?;
    Some((x, y, index))
}

fn native_vulkan_scene_path_point(current: [f64; 2], x: f64, y: f64, relative: bool) -> [f64; 2] {
    if relative {
        [current[0] + x, current[1] + y]
    } else {
        [x, y]
    }
}

fn native_vulkan_scene_reflected_control_point(
    current: [f64; 2],
    previous_control: Option<[f64; 2]>,
) -> [f64; 2] {
    previous_control
        .map(|control| [current[0] * 2.0 - control[0], current[1] * 2.0 - control[1]])
        .unwrap_or(current)
}

fn native_vulkan_scene_push_cubic_curve_points(
    points: &mut Vec<[f64; 2]>,
    start: [f64; 2],
    control_1: [f64; 2],
    control_2: [f64; 2],
    end: [f64; 2],
) -> Option<()> {
    for segment in 1..=SCENE_FULL_PATH_CURVE_SEGMENTS {
        let t = segment as f64 / SCENE_FULL_PATH_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let x = inverse.powi(3) * start[0]
            + 3.0 * inverse.powi(2) * t * control_1[0]
            + 3.0 * inverse * t.powi(2) * control_2[0]
            + t.powi(3) * end[0];
        let y = inverse.powi(3) * start[1]
            + 3.0 * inverse.powi(2) * t * control_1[1]
            + 3.0 * inverse * t.powi(2) * control_2[1]
            + t.powi(3) * end[1];
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        points.push([x, y]);
    }
    Some(())
}

fn native_vulkan_scene_push_quadratic_curve_points(
    points: &mut Vec<[f64; 2]>,
    start: [f64; 2],
    control: [f64; 2],
    end: [f64; 2],
) -> Option<()> {
    for segment in 1..=SCENE_FULL_PATH_CURVE_SEGMENTS {
        let t = segment as f64 / SCENE_FULL_PATH_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let x = inverse.powi(2) * start[0] + 2.0 * inverse * t * control[0] + t.powi(2) * end[0];
        let y = inverse.powi(2) * start[1] + 2.0 * inverse * t * control[1] + t.powi(2) * end[1];
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        points.push([x, y]);
    }
    Some(())
}

fn native_vulkan_scene_path_arc_flag(value: f64) -> Option<bool> {
    if (value - 0.0).abs() < f64::EPSILON {
        Some(false)
    } else if (value - 1.0).abs() < f64::EPSILON {
        Some(true)
    } else {
        None
    }
}

fn native_vulkan_scene_push_arc_points(
    points: &mut Vec<[f64; 2]>,
    start: [f64; 2],
    rx: f64,
    ry: f64,
    x_axis_rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    end: [f64; 2],
) -> Option<()> {
    if !rx.is_finite()
        || !ry.is_finite()
        || !x_axis_rotation_deg.is_finite()
        || !start[0].is_finite()
        || !start[1].is_finite()
        || !end[0].is_finite()
        || !end[1].is_finite()
    {
        return None;
    }
    if (start[0] - end[0]).abs() < f64::EPSILON && (start[1] - end[1]).abs() < f64::EPSILON {
        return Some(());
    }
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx <= f64::EPSILON || ry <= f64::EPSILON {
        points.push(end);
        return Some(());
    }

    let phi = x_axis_rotation_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();
    let dx = (start[0] - end[0]) * 0.5;
    let dy = (start[1] - end[1]) * 0.5;
    let x1_prime = cos_phi.mul_add(dx, sin_phi * dy);
    let y1_prime = (-sin_phi).mul_add(dx, cos_phi * dy);

    let radius_scale = x1_prime.powi(2) / rx.powi(2) + y1_prime.powi(2) / ry.powi(2);
    if radius_scale > 1.0 {
        let scale = radius_scale.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let rx_sq = rx.powi(2);
    let ry_sq = ry.powi(2);
    let x1_prime_sq = x1_prime.powi(2);
    let y1_prime_sq = y1_prime.powi(2);
    let denominator = rx_sq * y1_prime_sq + ry_sq * x1_prime_sq;
    if denominator <= f64::EPSILON || !denominator.is_finite() {
        points.push(end);
        return Some(());
    }
    let numerator = (rx_sq * ry_sq - rx_sq * y1_prime_sq - ry_sq * x1_prime_sq).max(0.0);
    let center_scale =
        if large_arc == sweep { -1.0 } else { 1.0 } * (numerator / denominator).sqrt();
    let cx_prime = center_scale * rx * y1_prime / ry;
    let cy_prime = center_scale * -ry * x1_prime / rx;
    let cx = cos_phi.mul_add(cx_prime, -sin_phi * cy_prime) + (start[0] + end[0]) * 0.5;
    let cy = sin_phi.mul_add(cx_prime, cos_phi * cy_prime) + (start[1] + end[1]) * 0.5;

    let start_vector = [(x1_prime - cx_prime) / rx, (y1_prime - cy_prime) / ry];
    let end_vector = [(-x1_prime - cx_prime) / rx, (-y1_prime - cy_prime) / ry];
    let start_angle = native_vulkan_scene_vector_angle([1.0, 0.0], start_vector)?;
    let mut sweep_angle = native_vulkan_scene_vector_angle(start_vector, end_vector)?;
    if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f64::consts::TAU;
    } else if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f64::consts::TAU;
    }

    let segment_count = ((sweep_angle.abs() / (std::f64::consts::FRAC_PI_2))
        * SCENE_FULL_PATH_ARC_SEGMENTS_PER_QUARTER as f64)
        .ceil()
        .max(1.0) as usize;
    for segment in 1..=segment_count {
        let t = segment as f64 / segment_count as f64;
        let theta = start_angle + sweep_angle * t;
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();
        let x = cx + rx * cos_phi * cos_theta - ry * sin_phi * sin_theta;
        let y = cy + rx * sin_phi * cos_theta + ry * cos_phi * sin_theta;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        points.push([x, y]);
    }
    Some(())
}

fn native_vulkan_scene_vector_angle(from: [f64; 2], to: [f64; 2]) -> Option<f64> {
    let cross = from[0] * to[1] - from[1] * to[0];
    let dot = from[0] * to[0] + from[1] * to[1];
    if !cross.is_finite() || !dot.is_finite() {
        return None;
    }
    Some(cross.atan2(dot))
}

fn native_vulkan_scene_polygon_is_convex(points: &[[f64; 2]]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let c = points[(index + 2) % points.len()];
        let ab = [b[0] - a[0], b[1] - a[1]];
        let bc = [c[0] - b[0], c[1] - b[1]];
        let cross = ab[0].mul_add(bc[1], -ab[1] * bc[0]);
        if cross.abs() <= f64::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    sign != 0.0
}

fn native_vulkan_scene_triangulate_simple_polygon(points: &[[f64; 2]]) -> Option<Vec<u32>> {
    if points.len() < 3 || points.len() > u32::MAX as usize {
        return None;
    }
    let area = native_vulkan_scene_polygon_signed_area(points);
    if area.abs() <= f64::EPSILON {
        return None;
    }
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    if area < 0.0 {
        remaining.reverse();
    }
    let mut triangles = Vec::with_capacity((points.len().saturating_sub(2)) * 3);

    while remaining.len() > 3 {
        let mut ear_index = None;
        for index in 0..remaining.len() {
            let prev_index = remaining[(index + remaining.len() - 1) % remaining.len()];
            let curr_index = remaining[index];
            let next_index = remaining[(index + 1) % remaining.len()];
            if !native_vulkan_scene_triangle_is_counter_clockwise(
                points[prev_index],
                points[curr_index],
                points[next_index],
            ) {
                continue;
            }
            let contains_point = remaining.iter().copied().any(|candidate_index| {
                candidate_index != prev_index
                    && candidate_index != curr_index
                    && candidate_index != next_index
                    && native_vulkan_scene_point_in_triangle(
                        points[candidate_index],
                        points[prev_index],
                        points[curr_index],
                        points[next_index],
                    )
            });
            if !contains_point {
                ear_index = Some(index);
                triangles.extend_from_slice(&[
                    prev_index as u32,
                    curr_index as u32,
                    next_index as u32,
                ]);
                break;
            }
        }
        let ear_index = ear_index?;
        remaining.remove(ear_index);
    }

    triangles.extend_from_slice(&[
        remaining[0] as u32,
        remaining[1] as u32,
        remaining[2] as u32,
    ]);
    Some(triangles)
}

fn native_vulkan_scene_polygon_signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        area += a[0].mul_add(b[1], -b[0] * a[1]);
    }
    area * 0.5
}

fn native_vulkan_scene_triangle_is_counter_clockwise(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ac = [c[0] - a[0], c[1] - a[1]];
    ab[0].mul_add(ac[1], -ab[1] * ac[0]) > f64::EPSILON
}

fn native_vulkan_scene_point_in_triangle(
    point: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> bool {
    let area = |p0: [f64; 2], p1: [f64; 2], p2: [f64; 2]| {
        (p1[0] - p0[0]).mul_add(p2[1] - p0[1], -(p1[1] - p0[1]) * (p2[0] - p0[0]))
    };
    let ab = area(a, b, point);
    let bc = area(b, c, point);
    let ca = area(c, a, point);
    ab >= -f64::EPSILON && bc >= -f64::EPSILON && ca >= -f64::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::{SceneEffectFbo, SceneMesh, SceneMeshVertex};
    use crate::core::{FitMode, SceneBlendMode, ScenePathFillRule, SceneSize, SceneTextureRegion};
    use crate::renderer::SceneRenderTextureSlot;
    use crate::renderer::native_vulkan::present::render_plan::NativeVulkanSceneUnsupportedLayer;
    use crate::renderer::native_vulkan::present::render_plan::native_vulkan_scene_effect_uv_space_from_parts;
    use std::collections::BTreeMap;

    fn texture_slot_bindings(
        resources: &[u32],
    ) -> Vec<NativeVulkanSceneTextureSlotResourceBinding> {
        resources
            .iter()
            .copied()
            .enumerate()
            .map(
                |(slot, resource_index)| NativeVulkanSceneTextureSlotResourceBinding {
                    slot: slot.min(u32::MAX as usize) as u32,
                    resource_index,
                },
            )
            .collect()
    }

    fn draw_op(layer_index: usize, kind: NativeVulkanSceneDrawOpKind) -> NativeVulkanSceneDrawOp {
        NativeVulkanSceneDrawOp {
            layer_index,
            layer_id: format!("layer-{layer_index}"),
            kind,
            opacity: 1.0,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: SceneRenderAlphaTextureMode::Multiply,
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_uv_space: None,
            effect_motion: SceneNativeEffectMotion::default(),
            blend_mode: SceneBlendMode::Alpha,
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: FitMode::Cover,
            transform: SceneTransform::default(),
        }
    }

    #[test]
    fn draw_pass_plan_reports_fast_clear_color_ready() {
        let mut color = draw_op(0, NativeVulkanSceneDrawOpKind::ColorQuad);
        color.color = Some("#102030".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![color],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "fast-clear-color-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.recordable_op_count, 1);
        assert_eq!(pass_plan.recordable_quads.len(), 1);
        assert_eq!(pass_plan.recordable_quads[0].kind, "color-quad");
        assert!(!pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 0);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 0);
        assert_eq!(pass_plan.quad_index_buffer_bytes, 0);
        assert_eq!(
            pass_plan.recordable_quads[0].rgba,
            [16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 1.0]
        );
        assert_eq!(pass_plan.color_op_count, 1);
        assert_eq!(pass_plan.fast_clear_color.as_deref(), Some("#102030"));
        assert_eq!(pass_plan.recordable_quads[0].text, None);
    }

    #[test]
    fn draw_pass_plan_reports_resource_buckets_and_pending_backend() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));
        let text = draw_op(1, NativeVulkanSceneDrawOpKind::Text);
        let path = draw_op(2, NativeVulkanSceneDrawOpKind::Path);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image, text, path],
            unsupported_layers: Vec::new(),
            runtime_display_available: true,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(!pass_plan.backend_ready);
        assert_eq!(
            pass_plan.blocking_reason,
            Some("vulkan-draw-recording-not-implemented")
        );
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.video_op_count, 0);
        assert_eq!(pass_plan.text_op_count, 1);
        assert_eq!(pass_plan.path_op_count, 1);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/hero.png")]
        );
        assert!(pass_plan.requires_text_geometry);
        assert!(pass_plan.requires_path_tessellation);
        assert!(!pass_plan.requires_video_decode);
    }

    #[test]
    fn draw_pass_plan_reports_video_layer_bridge_ready() {
        let mut video = draw_op(0, NativeVulkanSceneDrawOpKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        video.fit = FitMode::Cover;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![video],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.video_op_count, 1);
        assert_eq!(
            pass_plan.required_video_resources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert!(pass_plan.requires_video_decode);
    }

    #[test]
    fn draw_pass_plan_reports_same_source_multi_video_bridge_ready() {
        let mut left = draw_op(0, NativeVulkanSceneDrawOpKind::Video);
        left.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        left.width = Some(640.0);
        left.height = Some(360.0);
        left.transform.x = 0.0;
        let mut right = draw_op(1, NativeVulkanSceneDrawOpKind::Video);
        right.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        right.width = Some(640.0);
        right.height = Some(360.0);
        right.transform.x = 640.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![left, right],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "multi-video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(pass_plan.video_op_count, 2);
        assert_eq!(
            pass_plan.required_video_resources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert!(pass_plan.video_recording_ready);
        assert_eq!(pass_plan.video_recording_steps.len(), 2);
        assert_eq!(pass_plan.video_vertices.len(), 8);
        assert_eq!(pass_plan.video_indices.len(), 12);
    }

    #[test]
    fn draw_pass_plan_reports_distinct_n_source_video_bridge_ready() {
        let mut sky = draw_op(0, NativeVulkanSceneDrawOpKind::Video);
        sky.source = Some(PathBuf::from("/tmp/sky.mp4"));
        sky.width = Some(1920.0);
        sky.height = Some(1080.0);
        let mut character = draw_op(1, NativeVulkanSceneDrawOpKind::Video);
        character.source = Some(PathBuf::from("/tmp/character.mp4"));
        character.width = Some(640.0);
        character.height = Some(1080.0);
        let mut effects = draw_op(2, NativeVulkanSceneDrawOpKind::Video);
        effects.source = Some(PathBuf::from("/tmp/effects.mp4"));
        effects.width = Some(1920.0);
        effects.height = Some(1080.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![sky, character, effects],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "multi-video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(pass_plan.video_op_count, 3);
        assert_eq!(pass_plan.required_video_resources.len(), 3);
        assert!(pass_plan.video_recording_ready);
        assert_eq!(pass_plan.video_recording_steps[0].resource_index, 0);
        assert_eq!(pass_plan.video_recording_steps[1].resource_index, 1);
        assert_eq!(pass_plan.video_recording_steps[2].resource_index, 2);
    }

    #[test]
    fn draw_pass_plan_reports_mixed_video_scene_bridge_ready() {
        let mut video = draw_op(0, NativeVulkanSceneDrawOpKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        let mut image = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/overlay.gtex"));
        image.width = Some(320.0);
        image.height = Some(180.0);
        let mut panel = draw_op(2, NativeVulkanSceneDrawOpKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(64.0);
        panel.height = Some(64.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![video, image, panel],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.video_op_count, 1);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(
            pass_plan.required_video_resources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert!(pass_plan.requires_video_decode);
    }

    #[test]
    fn draw_pass_plan_keeps_recordable_scene_subset_presentable_with_pending_layers() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.width = Some(663.0);
        image.height = Some(230.0);
        let mut panel = draw_op(1, NativeVulkanSceneDrawOpKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(64.0);
        panel.height = Some(64.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image, panel],
            unsupported_layers: vec![NativeVulkanSceneUnsupportedLayer {
                layer_index: 2,
                layer_id: "particle-layer".to_owned(),
                reason: "particle-layer-needs-scene-particle-runtime",
            }],
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(!pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "mixed-quad-sampled-image-recording-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
    }

    #[test]
    fn draw_pass_plan_reports_sampled_image_quad_payload() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));
        image.fit = FitMode::Contain;
        image.opacity = 0.75;
        image.color = Some("#000000".to_owned());
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.transform.x = 16.0;
        image.transform.y = 8.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "sampled-image-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 1);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 176);
        assert_eq!(pass_plan.sampled_image_index_buffer_bytes, 24);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2, 2, 1, 3]);
        let step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(
            super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                &step.material_pass.render_state
            ),
            "sampled-image-alpha-blend"
        );
        assert_eq!(
            step.material_pass.render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(step.source, PathBuf::from("/tmp/hero.png"));
        assert_eq!(step.fit, FitMode::Contain);
        assert_eq!(step.texture_slot_bindings, texture_slot_bindings(&[0]));
        assert_eq!(step.vertex_count, 4);
        assert_eq!(step.index_count, 6);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 4);
        assert_eq!(
            pass_plan.sampled_image_vertices[0].position,
            [-144.0, -82.0]
        );
        assert_eq!(pass_plan.sampled_image_vertices[3].position, [176.0, 98.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].opacity, 0.75);
        assert!(
            pass_plan
                .sampled_image_vertices
                .iter()
                .all(|vertex| vertex.tint == [0.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn draw_pass_plan_counts_effect_passes_across_image_and_non_image_layers() {
        let mut text = draw_op(0, NativeVulkanSceneDrawOpKind::Text);
        text.layer_id = "text-scroll".to_owned();
        text.text = Some("DREAMLIKE".to_owned());
        text.color = Some("#ffffff".to_owned());
        text.font_size = Some(16.0);
        text.width = Some(240.0);
        text.height = Some(48.0);
        text.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/scroll/effect.json".to_owned(),
            runtime: Some("wallpaper-engine-effect".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/scroll".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: Vec::new(),
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];

        let mut image = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        image.layer_id = "image-ripple".to_owned();
        image.source = Some(PathBuf::from("/tmp/source.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/source.gtex"),
            width: Some(320),
            height: Some(180),
        }];
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/waterripple/effect.json".to_owned(),
            runtime: Some("native-effect-motion".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/waterripple".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: Vec::new(),
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![text, image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.effect_pass_count, 2);
        assert_eq!(pass_plan.effect_pass_non_image_layer_count, 1);
        assert_eq!(
            pass_plan.effect_pass_kind_counts.get("scroll").copied(),
            Some(1)
        );
        assert_eq!(
            pass_plan
                .effect_pass_kind_counts
                .get("water-ripple")
                .copied(),
            Some(1)
        );
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .effect_kind_counts
                .get("water-ripple")
                .copied(),
            Some(1)
        );
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .effect_kind_counts
                .get("scroll"),
            None
        );
    }

    #[test]
    fn draw_pass_plan_reports_sampled_image_max_blend_pipeline() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/caustic.gtex"));
        image.blend_mode = SceneBlendMode::Max;
        image.width = Some(320.0);
        image.height = Some(180.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.sampled_image_recording_ready);
        let step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(
            step.material_pass.render_state.blend.mode,
            SceneBlendMode::Max
        );
        assert_eq!(
            step.material_pass.render_state.blend.equation.color_op,
            NativeVulkanSceneBlendOp::Max
        );
        assert_eq!(
            step.material_pass.render_state.blend.equation.dst_alpha,
            NativeVulkanSceneBlendFactor::OneMinusSrcAlpha
        );
        assert_eq!(
            super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                &step.material_pass.render_state
            ),
            "sampled-image-max-blend"
        );
    }

    #[test]
    fn draw_pass_plan_reports_sampled_image_multiply_and_screen_blend_pipelines() {
        let mut multiply = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        multiply.source = Some(PathBuf::from("/tmp/shadow.gtex"));
        multiply.blend_mode = SceneBlendMode::Multiply;
        multiply.width = Some(64.0);
        multiply.height = Some(64.0);
        let mut screen = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        screen.source = Some(PathBuf::from("/tmp/water.gtex"));
        screen.blend_mode = SceneBlendMode::Screen;
        screen.width = Some(64.0);
        screen.height = Some(64.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![multiply, screen],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(
            pass_plan
                .sampled_image_recording_steps
                .iter()
                .map(|step| {
                    (
                        step.material_pass.render_state.blend.mode,
                        super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                            &step.material_pass.render_state,
                        ),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (SceneBlendMode::Multiply, "sampled-image-multiply-blend"),
                (SceneBlendMode::Screen, "sampled-image-screen-blend"),
            ]
        );
    }

    #[test]
    fn draw_pass_plan_reports_solid_quad_screen_blend_pipeline() {
        let mut panel = draw_op(0, NativeVulkanSceneDrawOpKind::Rectangle);
        panel.color = Some("#003ca4".to_owned());
        panel.blend_mode = SceneBlendMode::Screen;
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![panel],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.quad_recording_ready);
        assert_eq!(
            pass_plan.recordable_quads[0].blend.mode,
            SceneBlendMode::Screen
        );
        assert_eq!(
            pass_plan.quad_recording_steps[0].blend.mode,
            SceneBlendMode::Screen
        );
        assert_eq!(
            super::blend::native_vulkan_scene_solid_quad_pipeline_label(
                pass_plan.quad_recording_steps[0].blend
            ),
            "solid-quad-screen-blend"
        );
    }

    #[test]
    fn draw_pass_plan_culls_sampled_images_outside_scene_bounds() {
        let mut visible = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        visible.source = Some(PathBuf::from("/tmp/visible.png"));
        visible.width = Some(16.0);
        visible.height = Some(16.0);
        visible.transform.x = 40.0;
        visible.transform.y = 40.0;
        let mut outside = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        outside.source = Some(PathBuf::from("/tmp/offscreen.png"));
        outside.width = Some(16.0);
        outside.height = Some(16.0);
        outside.transform.x = 140.0;
        outside.transform.y = 140.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: Some(SceneSize {
                width: 100,
                height: 100,
            }),
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![visible, outside],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_op_count, 2);
        assert_eq!(pass_plan.sampled_image_quads.len(), 2);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 4);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![PathBuf::from("/tmp/visible.png")]
        );
        assert_eq!(
            pass_plan.required_image_resources,
            vec![
                PathBuf::from("/tmp/visible.png"),
                PathBuf::from("/tmp/offscreen.png")
            ]
        );
    }

    #[test]
    fn draw_pass_plan_keeps_dynamic_topology_sampled_images_stable() {
        let mut visible = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        visible.source = Some(PathBuf::from("/tmp/visible.png"));
        visible.width = Some(16.0);
        visible.height = Some(16.0);
        visible.transform.x = 40.0;
        visible.transform.y = 40.0;
        let mut outside = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        outside.source = Some(PathBuf::from("/tmp/offscreen.png"));
        outside.width = Some(16.0);
        outside.height = Some(16.0);
        outside.transform.x = 140.0;
        outside.transform.y = 140.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: Some(SceneSize {
                width: 100,
                height: 100,
            }),
            scene_fit: FitMode::Cover,
            dynamic_topology_required: true,
            draw_ops: vec![visible, outside],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 2);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 8);
        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![
                PathBuf::from("/tmp/visible.png"),
                PathBuf::from("/tmp/offscreen.png")
            ]
        );
    }

    #[test]
    fn draw_pass_plan_reports_sampled_image_mesh_payload() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/puppet.gtex"));
        image.opacity = 0.8;
        image.width = Some(10.0);
        image.height = Some(10.0);
        image.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -1.0,
                    y: -1.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 3.0,
                    y: -1.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -1.0,
                    y: 2.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        }));
        image.transform.x = 10.0;
        image.transform.y = 20.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "sampled-image-recording-ready");
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 132);
        assert_eq!(pass_plan.sampled_image_index_buffer_bytes, 12);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2]);
        let step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(step.vertex_count, 3);
        assert_eq!(step.index_count, 3);
        assert_eq!(pass_plan.sampled_image_vertices[0].position, [9.0, 19.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].position, [9.0, 22.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].opacity, 0.8);
    }

    #[test]
    fn draw_pass_plan_uses_puppet_mesh_with_material_uv_for_opacity_effect() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.texture_slots = vec![
            SceneRenderTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            },
            SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/opacity-mask.gtex"),
                width: Some(331),
                height: Some(115),
            },
        ];
        image.alpha_texture_slot = Some(1);
        image.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        image.effect_uv_space = Some(NativeVulkanSceneEffectUvSpace {
            mapping: NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
                scale_u: 1.0,
                scale_v: 1.0,
                offset_u: 0.0,
                offset_v: 0.0,
            },
            width: 663.0,
            height: 230.0,
            texture_region: None,
            transform: Default::default(),
            bounds: None,
        });
        image.width = Some(663.0);
        image.height = Some(230.0);
        image.mesh = Some(Arc::new(SceneMesh {
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
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.sampled_image_recording_steps[0].vertex_count, 3);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].index_count, 3);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2]);
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [0.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].effect_uv, [0.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].effect_uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].effect_uv, [0.0, 1.0]);
    }

    #[test]
    fn draw_pass_plan_routes_opacity_effect_through_first_class_target() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.blend_mode = SceneBlendMode::Normal;
        image.texture_slots = vec![
            SceneRenderTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            },
            SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/opacity-mask.gtex"),
                width: Some(331),
                height: Some(115),
            },
        ];
        image.alpha_texture_slot = Some(1);
        image.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        image.effect_uv_space = Some(NativeVulkanSceneEffectUvSpace {
            mapping: NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
                scale_u: 1.0,
                scale_v: 1.0,
                offset_u: 0.0,
                offset_v: 0.0,
            },
            width: 663.0,
            height: 230.0,
            texture_region: None,
            transform: Default::default(),
            bounds: None,
        });
        image.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("wallpaper-engine-effect".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            texture_slots: image.texture_slots.clone(),
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        image.width = Some(663.0);
        image.height = Some(230.0);
        image.mesh = Some(Arc::new(SceneMesh {
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
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.sampled_image_effect_targets.len(), 1);
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].effect_target_index,
            0
        );
        assert_eq!(pass_plan.sampled_image_effect_targets[0].width, 663);
        assert_eq!(pass_plan.sampled_image_effect_targets[0].height, 230);
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].we_graph_chain_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].we_graph_target_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].we_graph_endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::FirstClassEffectTarget)
        );
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .first_class_target_chain_count,
            1
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.target_count, 1);
        assert_eq!(pass_plan.sampled_image_we_graph_plan.step_count, 2);
        assert_eq!(pass_plan.sampled_image_sources.len(), 2);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 2);
        let base_step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(
            base_step.render_target,
            NativeVulkanSceneSampledImageRenderTarget::EffectTarget {
                target_index: 0,
                clear: true
            }
        );
        assert_eq!(base_step.texture_slot_bindings, texture_slot_bindings(&[0]));
        assert_eq!(base_step.we_graph_chain_index, Some(0));
        assert_eq!(base_step.we_graph_step_index, Some(0));
        assert_eq!(base_step.we_graph_input_target_index, None);
        assert_eq!(base_step.we_graph_output_target_index, Some(0));
        assert_eq!(base_step.material_pass.alpha_texture_slot, None);
        assert_eq!(
            base_step.material_pass.render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(base_step.vertex_count, 3);
        assert_eq!(base_step.index_count, 3);
        let final_step = &pass_plan.sampled_image_recording_steps[1];
        assert_eq!(
            final_step.render_target,
            NativeVulkanSceneSampledImageRenderTarget::Swapchain
        );
        assert_eq!(
            final_step.texture_slot_bindings,
            texture_slot_bindings(&[2, 1])
        );
        assert_eq!(final_step.we_graph_chain_index, Some(0));
        assert_eq!(final_step.we_graph_step_index, Some(1));
        assert_eq!(final_step.we_graph_input_target_index, Some(0));
        assert_eq!(final_step.we_graph_output_target_index, None);
        assert_eq!(final_step.material_pass.alpha_texture_slot, Some(1));
        assert_eq!(
            final_step.material_pass.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Coverage
        );
        let graph_base_bindings = &pass_plan.sampled_image_we_graph_plan.steps[0].texture_bindings;
        assert_eq!(graph_base_bindings.len(), 1);
        assert_eq!(
            graph_base_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::SourceTexture
        );
        let graph_final_bindings = &pass_plan.sampled_image_we_graph_plan.steps[1].texture_bindings;
        assert_eq!(graph_final_bindings.len(), 2);
        assert_eq!(
            graph_final_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget
        );
        assert_eq!(graph_final_bindings[0].target_index, Some(0));
        assert_eq!(
            graph_final_bindings[0].endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::FirstClassEffectTarget)
        );
        assert_eq!(
            graph_final_bindings[1].source_path,
            Some(PathBuf::from("/tmp/opacity-mask.gtex"))
        );
        assert_eq!(
            final_step.material_pass.render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(final_step.vertex_count, 4);
        assert_eq!(final_step.index_count, 6);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 7);
        assert_eq!(pass_plan.sampled_image_indices.len(), 9);
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].uv, [1.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].uv, [0.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[6].uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].effect_uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[6].effect_uv, [1.0, 0.0]);
    }

    #[test]
    fn draw_pass_plan_binds_direct_effect_pass_texture_slots() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(663),
            height: Some(230),
        }];
        image.alpha_texture_slot = Some(1);
        image.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        image.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("native-opacity-mask".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: None,
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/opacity-mask.gtex"),
                width: Some(331),
                height: Some(115),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        image.width = Some(663.0);
        image.height = Some(230.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![
                PathBuf::from("/tmp/eye.gtex"),
                PathBuf::from("/tmp/opacity-mask.gtex")
            ]
        );
        assert_eq!(pass_plan.sampled_image_effect_targets.len(), 1);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 2);
        let base_step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(base_step.texture_slot_bindings, texture_slot_bindings(&[0]));
        assert_eq!(base_step.material_pass.alpha_texture_slot, None);
        let final_step = &pass_plan.sampled_image_recording_steps[1];
        assert_eq!(
            final_step.texture_slot_bindings,
            texture_slot_bindings(&[2, 1])
        );
        assert_eq!(final_step.material_pass.alpha_texture_slot, Some(1));
        assert_eq!(
            final_step.material_pass.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Coverage
        );
    }

    #[test]
    fn draw_pass_plan_preserves_we_effect_bind_overrides_as_graph_bindings() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/source.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/source.gtex"),
            width: Some(320),
            height: Some(180),
        }];
        image.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/custom/effect.json".to_owned(),
            runtime: Some("wallpaper-engine-effect".to_owned()),
            pass_index: 0,
            command: Some("draw".to_owned()),
            source: Some("previous".to_owned()),
            target: Some("_rt_Custom".to_owned()),
            binds: BTreeMap::from([
                (0, "previous".to_owned()),
                (2, "_rt_CustomNormal".to_owned()),
            ]),
            fbos: Default::default(),
            shader: Some("effects/custom".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: None,
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/normal.gtex"),
                width: Some(64),
                height: Some(64),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        image.blend_mode = SceneBlendMode::Alpha;
        image.width = Some(320.0);
        image.height = Some(180.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);
        let effect_step = &pass_plan.sampled_image_we_graph_plan.steps[1];

        assert_eq!(pass_plan.sampled_image_we_graph_plan.step_count, 2);
        assert_eq!(effect_step.pass.command.as_deref(), Some("draw"));
        assert_eq!(effect_step.pass.source.as_deref(), Some("previous"));
        assert_eq!(effect_step.pass.target_name.as_deref(), Some("_rt_Custom"));
        assert_eq!(effect_step.texture_bindings.len(), 2);
        assert_eq!(effect_step.texture_bindings[0].slot, 0);
        assert_eq!(
            effect_step.texture_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget
        );
        assert_eq!(
            effect_step.texture_bindings[0].bind_name.as_deref(),
            Some("previous")
        );
        assert_eq!(effect_step.texture_bindings[0].target_index, Some(0));
        assert_eq!(effect_step.texture_bindings[1].slot, 2);
        assert_eq!(
            effect_step.texture_bindings[1].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::NamedFboBind
        );
        assert_eq!(
            effect_step.texture_bindings[1].bind_name.as_deref(),
            Some("_rt_CustomNormal")
        );
        assert_eq!(effect_step.texture_bindings[1].target_index, None);
        assert_eq!(effect_step.texture_bindings[1].source_path, None);
    }

    #[test]
    fn draw_pass_plan_resolves_named_fbo_bindings_to_graph_targets() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/source.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/source.gtex"),
            width: Some(400),
            height: Some(200),
        }];
        image.image_effect_passes = vec![
            crate::renderer::SceneRenderImageEffectPass {
                effect_file: "effects/custom/effect.json".to_owned(),
                runtime: Some("wallpaper-engine-effect".to_owned()),
                pass_index: 0,
                command: Some("draw".to_owned()),
                source: Some("previous".to_owned()),
                target: Some("_rt_Aux".to_owned()),
                binds: BTreeMap::from([(0, "previous".to_owned())]),
                fbos: vec![SceneEffectFbo {
                    name: "_rt_Aux".to_owned(),
                    format: Some("rgba8888".to_owned()),
                    scale: 0.5,
                    unique: true,
                }],
                shader: Some("effects/custom".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: Some("disabled".to_owned()),
                depthwrite: Some("disabled".to_owned()),
                cullmode: None,
                texture_slots: Vec::new(),
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            },
            crate::renderer::SceneRenderImageEffectPass {
                effect_file: "effects/custom/effect.json".to_owned(),
                runtime: Some("wallpaper-engine-effect".to_owned()),
                pass_index: 1,
                command: Some("draw".to_owned()),
                source: Some("previous".to_owned()),
                target: None,
                binds: BTreeMap::from([(2, "_rt_Aux".to_owned())]),
                fbos: vec![SceneEffectFbo {
                    name: "_rt_Aux".to_owned(),
                    format: Some("rgba8888".to_owned()),
                    scale: 0.5,
                    unique: true,
                }],
                shader: Some("effects/custom2".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: Some("disabled".to_owned()),
                depthwrite: Some("disabled".to_owned()),
                cullmode: None,
                texture_slots: Vec::new(),
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            },
        ];
        image.blend_mode = SceneBlendMode::Alpha;
        image.width = Some(400.0);
        image.height = Some(200.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);
        let named_target = pass_plan
            .sampled_image_we_graph_plan
            .targets
            .iter()
            .find(|target| target.name.as_deref() == Some("_rt_Aux"))
            .expect("named FBO graph target");
        let second_effect = &pass_plan.sampled_image_we_graph_plan.steps[2];
        let named_binding = second_effect
            .texture_bindings
            .iter()
            .find(|binding| binding.slot == 2)
            .expect("named FBO binding");

        assert_eq!(
            named_target.endpoint,
            NativeVulkanSceneWeImagePassEndpoint::NamedFbo
        );
        assert_eq!(named_target.format.as_deref(), Some("rgba8888"));
        assert_eq!(named_target.scale, Some(0.5));
        assert!(named_target.unique);
        assert_eq!(named_target.width, 200);
        assert_eq!(named_target.height, 100);
        assert_eq!(named_target.write_count, 1);
        assert!(named_target.sampled_by_following_pass);
        assert_eq!(second_effect.pass.input_name.as_deref(), Some("_rt_Aux"));
        assert_eq!(
            named_binding.source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::NamedFboBind
        );
        assert_eq!(named_binding.target_index, Some(named_target.target_index));
        assert_eq!(
            named_binding.endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::NamedFbo)
        );
        assert_eq!(named_binding.resolution, Some([200, 100]));
    }

    #[test]
    fn draw_pass_plan_suppresses_unimplemented_water_effect_source_quad() {
        let mut water = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        water.layer_id = "water-effect-carrier".to_owned();
        water.source = Some(PathBuf::from("/tmp/water-source.gtex"));
        water.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/water-source.gtex"),
            width: Some(3450),
            height: Some(3000),
        }];
        water.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/waterripple/effect.json".to_owned(),
            runtime: Some("native-effect-motion".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/waterripple".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/waterripplenormal.gtex"),
                width: Some(512),
                height: Some(512),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        water.blend_mode = SceneBlendMode::Modulate;
        water.width = Some(3450.0);
        water.height = Some(3000.0);

        let mut hero = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        hero.layer_id = "hero".to_owned();
        hero.source = Some(PathBuf::from("/tmp/hero.gtex"));
        hero.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/hero.gtex"),
            width: Some(512),
            height: Some(512),
        }];
        hero.width = Some(512.0);
        hero.height = Some(512.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![water, hero],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);
        let chain = native_vulkan_scene_we_image_pass_chain(&pass_plan.sampled_image_quads[0])
            .expect("water carrier should have a WE image pass chain");

        assert!(pass_plan.backend_ready);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_quads.len(), 2);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].layer_id, "hero");
        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![PathBuf::from("/tmp/hero.gtex")]
        );
        assert_eq!(pass_plan.sampled_image_effect_targets.len(), 0);
        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor
        );
        assert!(chain.local_target_required);
        assert!(chain.color_blend_passthrough);
        assert!(chain.first_pass_blend_moved_to_final);
        assert!(!chain.raw_direct_composite_allowed);
        assert_eq!(chain.final_scene_blend_mode, SceneBlendMode::Modulate);
        assert_eq!(
            chain
                .passes
                .iter()
                .map(|pass| pass.role)
                .collect::<Vec<_>>(),
            vec![
                NativeVulkanSceneWeImagePassRole::BaseMaterial,
                NativeVulkanSceneWeImagePassRole::EffectMaterial,
                NativeVulkanSceneWeImagePassRole::ColorBlendPassthrough
            ]
        );
        assert_eq!(
            chain.passes.last().map(|pass| pass.target),
            Some(NativeVulkanSceneWeImagePassEndpoint::Scene)
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.chain_count, 1);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.suppressed_chain_count,
            1
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.target_count, 2);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.final_scene_step_count,
            1
        );
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .targets
                .iter()
                .map(|target| target.endpoint)
                .collect::<Vec<_>>(),
            vec![
                NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain,
                NativeVulkanSceneWeImagePassEndpoint::ImageLocalSub
            ]
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.step_count, 3);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[0].output_target_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[1].input_target_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[1].output_target_index,
            Some(1)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2].input_target_index,
            Some(1)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2].output_target_index,
            None
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2].pass.role,
            NativeVulkanSceneWeImagePassRole::ColorBlendPassthrough
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2]
                .pass
                .scene_blend_mode,
            SceneBlendMode::Modulate
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[0]
                .pass
                .render_state
                .blend
                .mode,
            SceneBlendMode::Normal
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[1]
                .pass
                .render_state
                .blend
                .mode,
            SceneBlendMode::Normal
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2]
                .pass
                .render_state
                .blend
                .mode,
            SceneBlendMode::Modulate
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2]
                .pass
                .render_state
                .blend
                .equation
                .src_color,
            NativeVulkanSceneBlendFactor::DstColor
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[1]
                .pass
                .texture_slots[0]
                .slot,
            2
        );
        let base_bindings = &pass_plan.sampled_image_we_graph_plan.steps[0].texture_bindings;
        assert_eq!(base_bindings.len(), 1);
        assert_eq!(base_bindings[0].slot, 0);
        assert_eq!(
            base_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::SourceTexture
        );
        assert_eq!(
            base_bindings[0].source_path,
            Some(PathBuf::from("/tmp/water-source.gtex"))
        );
        assert_eq!(base_bindings[0].resolution, Some([3450, 3000]));
        let ripple_bindings = &pass_plan.sampled_image_we_graph_plan.steps[1].texture_bindings;
        assert_eq!(ripple_bindings.len(), 2);
        assert_eq!(ripple_bindings[0].slot, 0);
        assert_eq!(
            ripple_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget
        );
        assert_eq!(ripple_bindings[0].target_index, Some(0));
        assert_eq!(
            ripple_bindings[0].endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain)
        );
        assert_eq!(ripple_bindings[0].resolution, Some([3450, 3000]));
        assert_eq!(ripple_bindings[1].slot, 2);
        assert_eq!(
            ripple_bindings[1].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::PassTextureSlot
        );
        assert_eq!(
            ripple_bindings[1].source_path,
            Some(PathBuf::from("/tmp/waterripplenormal.gtex"))
        );
        assert_eq!(ripple_bindings[1].resolution, Some([512, 512]));
        let passthrough_bindings = &pass_plan.sampled_image_we_graph_plan.steps[2].texture_bindings;
        assert_eq!(passthrough_bindings.len(), 1);
        assert_eq!(passthrough_bindings[0].slot, 0);
        assert_eq!(
            passthrough_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget
        );
        assert_eq!(passthrough_bindings[0].target_index, Some(1));
        assert_eq!(
            passthrough_bindings[0].endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::ImageLocalSub)
        );
    }

    #[test]
    fn draw_pass_plan_suppresses_plain_unimplemented_water_ripple_source_quad() {
        let mut water = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        water.layer_id = "plain-water-ripple-carrier".to_owned();
        water.source = Some(PathBuf::from("/tmp/plain-water-source.gtex"));
        water.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/plain-water-source.gtex"),
            width: Some(1024),
            height: Some(512),
        }];
        water.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/waterripple/effect.json".to_owned(),
            runtime: Some("native-effect-motion".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/waterripple".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/waterripplenormal.gtex"),
                width: Some(512),
                height: Some(512),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        water.blend_mode = SceneBlendMode::Alpha;
        water.width = Some(1024.0);
        water.height = Some(512.0);

        let mut hero = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        hero.layer_id = "hero".to_owned();
        hero.source = Some(PathBuf::from("/tmp/hero.gtex"));
        hero.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/hero.gtex"),
            width: Some(512),
            height: Some(512),
        }];
        hero.width = Some(512.0);
        hero.height = Some(512.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![water, hero],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);
        let chain = native_vulkan_scene_we_image_pass_chain(&pass_plan.sampled_image_quads[0])
            .expect("plain water ripple carrier should have a WE image pass chain");

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].layer_id, "hero");
        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor
        );
        assert!(!chain.color_blend_passthrough);
        assert!(!chain.raw_direct_composite_allowed);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.suppressed_chain_count,
            1
        );
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .temporary_raw_fallback_chain_count,
            0
        );
    }

    #[test]
    fn draw_pass_plan_keeps_alpha_waterwaves_character_quad() {
        let mut hair = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        hair.layer_id = "character-hair-waterwaves".to_owned();
        hair.source = Some(PathBuf::from("/tmp/hair.gtex"));
        hair.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/hair.gtex"),
            width: Some(2318),
            height: Some(1794),
        }];
        hair.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/waterwaves/effect.json".to_owned(),
            runtime: Some("native-effect-motion".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/waterwaves".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/waterwaves-mask.gtex"),
                width: Some(1159),
                height: Some(897),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        hair.blend_mode = SceneBlendMode::Alpha;
        hair.width = Some(2318.0);
        hair.height = Some(1794.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![hair],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);
        let chain = native_vulkan_scene_we_image_pass_chain(&pass_plan.sampled_image_quads[0])
            .expect("waterwaves character layer should have a WE image pass chain");

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(
            pass_plan.sampled_image_recording_steps[0].layer_id,
            "character-hair-waterwaves"
        );
        assert_eq!(
            pass_plan.sampled_image_recording_steps[0]
                .material_pass
                .render_state
                .blend
                .mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(
            super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                &pass_plan.sampled_image_recording_steps[0]
                    .material_pass
                    .render_state
            ),
            "sampled-image-alpha-blend"
        );
        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![
                PathBuf::from("/tmp/hair.gtex"),
                PathBuf::from("/tmp/waterwaves-mask.gtex")
            ]
        );
        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::TemporaryRawFallback
        );
        assert!(chain.local_target_required);
        assert!(!chain.color_blend_passthrough);
        assert!(chain.raw_direct_composite_allowed);
        assert_eq!(chain.final_scene_blend_mode, SceneBlendMode::Alpha);
        assert_eq!(pass_plan.sampled_image_we_graph_plan.chain_count, 1);
        assert_eq!(
            pass_plan
                .sampled_image_we_graph_plan
                .temporary_raw_fallback_chain_count,
            1
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.suppressed_chain_count,
            0
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.target_count, 1);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.targets[0].endpoint,
            NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.final_scene_step_count,
            1
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.step_count, 2);
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[0].output_target_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[1].input_target_index,
            Some(0)
        );
    }

    #[test]
    fn draw_pass_plan_routes_iris_effect_through_first_class_target() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(663),
            height: Some(230),
        }];
        image.image_effect_passes = vec![crate::renderer::SceneRenderImageEffectPass {
            effect_file: "effects/iris/effect.json".to_owned(),
            runtime: Some("native-iris-mask".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/iris".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/iris-mask.gtex"),
                width: Some(331),
                height: Some(115),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        image.width = Some(663.0);
        image.height = Some(230.0);
        image.mesh = Some(Arc::new(SceneMesh {
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
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.sampled_image_effect_targets.len(), 1);
        assert_eq!(pass_plan.sampled_image_effect_targets[0].width, 663);
        assert_eq!(pass_plan.sampled_image_effect_targets[0].height, 230);
        assert_eq!(pass_plan.sampled_image_sources.len(), 2);
        assert_eq!(
            pass_plan.sampled_image_sources,
            vec![
                PathBuf::from("/tmp/eye.gtex"),
                PathBuf::from("/tmp/iris-mask.gtex")
            ]
        );
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 2);
        let base_step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(
            base_step.render_target,
            NativeVulkanSceneSampledImageRenderTarget::EffectTarget {
                target_index: 0,
                clear: true
            }
        );
        assert_eq!(base_step.texture_slot_bindings, texture_slot_bindings(&[0]));
        assert_eq!(base_step.material_pass.alpha_texture_slot, None);
        assert_eq!(
            base_step.material_pass.render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(
            super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                &base_step.material_pass.render_state
            ),
            "sampled-image-alpha-blend"
        );
        assert_eq!(base_step.vertex_count, 3);
        assert_eq!(base_step.index_count, 3);
        let final_step = &pass_plan.sampled_image_recording_steps[1];
        assert_eq!(
            final_step.render_target,
            NativeVulkanSceneSampledImageRenderTarget::Swapchain
        );
        assert_eq!(
            final_step.texture_slot_bindings,
            texture_slot_bindings(&[2, 1])
        );
        assert_eq!(final_step.material_pass.alpha_texture_slot, Some(1));
        assert_eq!(
            final_step.material_pass.alpha_texture_mode,
            SceneRenderAlphaTextureMode::Iris
        );
        assert_eq!(
            final_step.material_pass.render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(
            super::blend::native_vulkan_scene_sampled_image_pipeline_label(
                &final_step.material_pass.render_state
            ),
            "sampled-image-alpha-blend"
        );
        assert_eq!(final_step.vertex_count, 4);
        assert_eq!(final_step.index_count, 6);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 7);
        assert_eq!(pass_plan.sampled_image_indices.len(), 9);
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].uv, [1.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].uv, [0.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[6].uv, [1.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].effect_uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[6].effect_uv, [1.0, 0.0]);
    }

    #[test]
    fn draw_pass_plan_does_not_fake_graph_link_for_collapsed_first_class_chain() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(663),
            height: Some(230),
        }];
        image.image_effect_passes = vec![
            crate::renderer::SceneRenderImageEffectPass {
                effect_file: "effects/iris/effect.json".to_owned(),
                runtime: Some("native-iris-mask".to_owned()),
                pass_index: 0,
                command: None,
                source: None,
                target: None,
                binds: Default::default(),
                fbos: Default::default(),
                shader: Some("effects/iris".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                texture_slots: vec![SceneRenderTextureSlot {
                    slot: 1,
                    source: PathBuf::from("/tmp/iris-mask.gtex"),
                    width: Some(331),
                    height: Some(115),
                }],
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            },
            crate::renderer::SceneRenderImageEffectPass {
                effect_file: "effects/waterripple/effect.json".to_owned(),
                runtime: Some("native-effect-motion".to_owned()),
                pass_index: 1,
                command: None,
                source: None,
                target: None,
                binds: Default::default(),
                fbos: Default::default(),
                shader: Some("effects/waterripple".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: Some("disabled".to_owned()),
                depthwrite: Some("disabled".to_owned()),
                cullmode: Some("nocull".to_owned()),
                texture_slots: vec![SceneRenderTextureSlot {
                    slot: 2,
                    source: PathBuf::from("/tmp/waterripplenormal.gtex"),
                    width: Some(512),
                    height: Some(512),
                }],
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            },
        ];
        image.width = Some(663.0);
        image.height = Some(230.0);

        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.sampled_image_effect_targets.len(), 1);
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].effect_target_index,
            0
        );
        assert_eq!(
            pass_plan.sampled_image_effect_targets[0].we_graph_endpoint,
            Some(NativeVulkanSceneWeImagePassEndpoint::FirstClassEffectTarget)
        );
        assert_eq!(pass_plan.sampled_image_we_graph_plan.target_count, 2);
        assert_eq!(pass_plan.sampled_image_we_graph_plan.step_count, 3);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 2);
        assert_eq!(
            pass_plan.sampled_image_recording_steps[0].we_graph_step_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_recording_steps[0].we_graph_output_target_index,
            Some(0)
        );
        assert_eq!(
            pass_plan.sampled_image_recording_steps[1].render_target,
            NativeVulkanSceneSampledImageRenderTarget::Swapchain
        );
        assert_eq!(
            pass_plan.sampled_image_recording_steps[1].we_graph_step_index,
            None
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2]
                .pass
                .effect_kind,
            Some(NativeVulkanSceneEffectKind::WaterRipple)
        );
        assert_eq!(
            pass_plan.sampled_image_we_graph_plan.steps[2].input_target_index,
            Some(1)
        );
    }

    #[test]
    fn draw_pass_plan_maps_effect_uv_to_mesh_bounds_when_explicitly_requested() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/eye.gtex"));
        image.width = Some(100.0);
        image.height = Some(100.0);
        image.mesh = Some(Arc::new(SceneMesh {
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
        image.effect_uv_space = Some(native_vulkan_scene_effect_uv_space_from_parts(
            image.width,
            image.height,
            image.mesh.as_deref(),
            None,
            image.transform,
        ));
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert_eq!(pass_plan.sampled_image_vertices.len(), 3);
        assert_eq!(pass_plan.sampled_image_vertices[0].effect_uv, [0.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].effect_uv, [1.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].effect_uv, [0.0, 0.0]);
    }

    #[test]
    fn draw_pass_plan_preserves_sampled_image_mesh_geometry() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/puppet.gtex"));
        image.width = Some(2.0);
        image.height = Some(2.0);
        image.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -0.5,
                    y: 0.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 1.5,
                    y: 0.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -0.5,
                    y: 0.5,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        }));
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 3);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2]);
        assert_eq!(pass_plan.sampled_image_vertices[0].position, [-0.5, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[1].position, [1.5, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[2].position, [-0.5, 0.5]);
    }

    #[test]
    fn draw_pass_plan_tessellates_effect_sampled_image_quads() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/water-hair.gtex"));
        image.width = Some(120.0);
        image.height = Some(60.0);
        image.effect_motion = SceneNativeEffectMotion {
            wave_x: 2.0,
            wave_y: 1.0,
            wave_direction_x: 1.0,
            wave_spatial_frequency: 0.1,
            wave_phase: 0.5,
            wave_count: 1,
            ..Default::default()
        };
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 169);
        assert_eq!(pass_plan.sampled_image_indices.len(), 864);
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 7436);
        assert_eq!(pass_plan.sampled_image_index_buffer_bytes, 3456);
        let step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(step.vertex_count, 169);
        assert_eq!(step.index_count, 864);
        assert_eq!(pass_plan.sampled_image_vertices[84].position, [0.0, 0.0]);
        assert_ne!(pass_plan.sampled_image_vertices[0].position, [-60.0, -30.0]);
    }

    #[test]
    fn draw_pass_plan_tessellates_effect_quad_meshes() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hair-strand.gtex"));
        image.width = Some(728.0);
        image.height = Some(757.0);
        image.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -364.0,
                    y: -378.5,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 364.0,
                    y: -378.5,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -364.0,
                    y: 378.5,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 364.0,
                    y: 378.5,
                    u: 1.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            skin: None,
            puppet_clips: Vec::new(),
        }));
        image.effect_motion = SceneNativeEffectMotion {
            sway_amplitude: 8.0,
            sway_spatial_frequency: 0.02,
            sway_phase: 1.0,
            sway_count: 1,
            ..Default::default()
        };
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 169);
        assert_eq!(pass_plan.sampled_image_indices.len(), 864);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].vertex_count, 169);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].index_count, 864);
        assert_eq!(pass_plan.sampled_image_vertices[84].position, [0.0, 0.0]);
    }

    #[test]
    fn draw_pass_plan_applies_effect_motion_to_complex_sampled_meshes() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/skirt-mesh.gtex"));
        image.width = Some(100.0);
        image.height = Some(100.0);
        image.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -50.0,
                    y: -50.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 50.0,
                    y: -50.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -50.0,
                    y: 50.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 50.0,
                    y: 50.0,
                    u: 1.0,
                    v: 1.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 0.0,
                    y: 0.0,
                    u: 0.5,
                    v: 0.5,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 4, 0, 4, 2, 1, 3, 4, 2, 4, 3],
            skin: None,
            puppet_clips: Vec::new(),
        }));
        image.effect_motion = SceneNativeEffectMotion {
            wave_x: 6.0,
            wave_y: 3.0,
            wave_direction_x: 1.0,
            wave_spatial_frequency: 0.05,
            wave_phase: 0.25,
            wave_count: 1,
            ..Default::default()
        };
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image.clone()],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 60);
        assert_eq!(pass_plan.sampled_image_indices.len(), 192);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].vertex_count, 60);
        assert_eq!(pass_plan.sampled_image_recording_steps[0].index_count, 192);
        assert_ne!(pass_plan.sampled_image_vertices[0].position, [-50.0, -50.0]);
        assert_eq!(pass_plan.sampled_image_vertices[4].position, [0.0, 0.0]);

        let mut dynamic_vertices = Vec::new();
        let quad = native_vulkan_scene_sampled_image_quad(&image).expect("sampled image quad");
        let dynamic_vertex_count =
            native_vulkan_scene_append_sampled_image_vertices(&quad, &mut dynamic_vertices)
                .expect("dynamic sampled mesh vertices");
        assert_eq!(dynamic_vertex_count, 60);
        assert_eq!(
            dynamic_vertices.len(),
            pass_plan.sampled_image_vertices.len()
        );
        assert_eq!(
            dynamic_vertices[0].position,
            pass_plan.sampled_image_vertices[0].position
        );
        assert_eq!(
            dynamic_vertices[4].position,
            pass_plan.sampled_image_vertices[4].position
        );
    }

    #[test]
    fn draw_pass_plan_uses_sampled_image_texture_region_uvs() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/atlas.gtex"));
        image.width = Some(2160.0);
        image.height = Some(1440.0);
        image.texture_region = Some(SceneTextureRegion {
            u_min: 2.0 / 3.0,
            v_min: 1.0 / 4.0,
            u_max: 1.0,
            v_max: 0.5,
            frame_index: 5,
            frame_count: 12,
            columns: 3,
            rows: 4,
            fps: Some(12.0),
            loop_playback: true,
        });
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 416,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(
            pass_plan.sampled_image_recording_steps[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 2.0 / 3.0,
                v_min: 1.0 / 4.0,
                u_max: 1.0,
                v_max: 0.5,
                frame_index: 5,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            })
        );
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [2.0 / 3.0, 0.5]);
        assert_eq!(pass_plan.sampled_image_vertices[3].uv, [1.0, 0.25]);
    }

    #[test]
    fn draw_pass_plan_treats_sized_scene_render_clear_as_background_for_atlas_scene() {
        let mut clear = draw_op(0, NativeVulkanSceneDrawOpKind::ColorQuad);
        clear.layer_id = "scene-render-clear-color".to_owned();
        clear.color = Some("#b3b3b3".to_owned());
        clear.width = Some(2160.0);
        clear.height = Some(1440.0);
        let mut image = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/atlas.gtex"));
        image.width = Some(2160.0);
        image.height = Some(1440.0);
        image.texture_region = Some(SceneTextureRegion {
            u_min: 0.0,
            v_min: 0.25,
            u_max: 1.0 / 3.0,
            v_max: 0.5,
            frame_index: 3,
            frame_count: 12,
            columns: 3,
            rows: 4,
            fps: Some(12.0),
            loop_playback: true,
        });
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 1000,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![clear, image],
            unsupported_layers: Vec::new(),
            runtime_display_available: true,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "clear-background-sampled-image-recording-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.clear_background_op_count, 1);
        assert_eq!(pass_plan.background_clear_color.as_deref(), Some("#b3b3b3"));
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps.len(), 0);
    }

    #[test]
    fn draw_pass_plan_reports_implicit_full_extent_sampled_image() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/fullscreen.png"));
        image.fit = FitMode::Cover;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "sampled-image-implicit-full-extent-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 0);
        assert!(pass_plan.sampled_image_implicit_full_extent_ready);
        assert!(!pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 0);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/fullscreen.png")]
        );
    }

    #[test]
    fn draw_pass_plan_reports_mixed_quad_and_implicit_full_extent_sampled_image() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/fullscreen.png"));
        image.fit = FitMode::Cover;
        let mut rectangle = draw_op(1, NativeVulkanSceneDrawOpKind::Rectangle);
        rectangle.color = Some("#102030".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![image, rectangle],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "mixed-quad-sampled-image-implicit-full-extent-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 0);
        assert!(pass_plan.sampled_image_implicit_full_extent_ready);
        assert!(!pass_plan.sampled_image_recording_ready);
        assert!(!pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/fullscreen.png")]
        );
    }

    #[test]
    fn draw_pass_plan_reports_mixed_quad_and_sampled_image_backend_ready() {
        let mut rectangle = draw_op(0, NativeVulkanSceneDrawOpKind::Rectangle);
        rectangle.color = Some("#102030".to_owned());
        rectangle.opacity = 0.8;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        let mut image = draw_op(1, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/overlay.png"));
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.opacity = 0.5;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![rectangle, image],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "mixed-quad-sampled-image-recording-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(!pass_plan.quad_recording_ready);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 96);
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 176);
    }

    #[test]
    fn draw_pass_plan_reports_recordable_rectangle_and_rounded_rectangle_payload() {
        let mut rectangle = draw_op(0, NativeVulkanSceneDrawOpKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.5;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.transform.x = 24.0;
        let mut rounded = draw_op(1, NativeVulkanSceneDrawOpKind::Rectangle);
        rounded.color = Some("#ffffff".to_owned());
        rounded.width = Some(120.0);
        rounded.height = Some(60.0);
        rounded.corner_radius = Some(8.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![rectangle, rounded],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.vector_shape_op_count, 2);
        assert_eq!(pass_plan.recordable_op_count, 2);
        assert_eq!(pass_plan.recordable_quads.len(), 2);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 2);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 41 * 24);
        assert_eq!(pass_plan.quad_index_buffer_bytes, 114 * 4);
        assert_eq!(
            super::blend::native_vulkan_scene_solid_quad_pipeline_label(
                pass_plan.quad_recording_steps[0].blend
            ),
            "solid-quad-alpha-blend"
        );
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "rectangle");
        assert_eq!(pass_plan.quad_recording_steps[1].kind, "rounded-rectangle");
        assert_eq!(pass_plan.quad_recording_steps[1].first_vertex, 4);
        assert_eq!(pass_plan.quad_recording_steps[1].vertex_count, 37);
        assert_eq!(pass_plan.quad_recording_steps[1].first_index, 6);
        assert_eq!(pass_plan.quad_recording_steps[1].index_count, 108);
        assert_eq!(pass_plan.quad_vertices.len(), 41);
        assert_eq!(pass_plan.quad_indices.len(), 114);
        assert_eq!(&pass_plan.quad_indices[0..6], &[0, 1, 2, 2, 1, 3]);
        let quad = &pass_plan.recordable_quads[0];
        assert_eq!(quad.kind, "rectangle");
        assert_eq!(quad.color, "#336699");
        assert_eq!(quad.rgba, [51.0 / 255.0, 102.0 / 255.0, 153.0 / 255.0, 0.5]);
        assert_eq!(quad.width, Some(640.0));
        assert_eq!(quad.height, Some(360.0));
        assert_eq!(quad.transform.x, 24.0);
        let rounded = &pass_plan.recordable_quads[1];
        assert_eq!(rounded.kind, "rounded-rectangle");
        assert_eq!(rounded.corner_radius, Some(8.0));
    }

    #[test]
    fn draw_pass_plan_records_ellipse_as_solid_geometry() {
        let mut ellipse = draw_op(0, NativeVulkanSceneDrawOpKind::Ellipse);
        ellipse.color = Some("#336699".to_owned());
        ellipse.opacity = 0.5;
        ellipse.width = Some(120.0);
        ellipse.height = Some(60.0);
        ellipse.transform.x = 10.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![ellipse],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "ellipse");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 49);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 144);
        assert_eq!(pass_plan.quad_vertices.len(), 49);
        assert_eq!(pass_plan.quad_indices.len(), 144);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 49 * 24);
        assert_eq!(pass_plan.quad_index_buffer_bytes, 144 * 4);
        assert_eq!(pass_plan.vector_shape_op_count, 1);
    }

    #[test]
    fn draw_pass_plan_records_simple_filled_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.path_data = Some("M0 0 L100 0 L100 50 L0 50 Z".to_owned());
        path.transform.x = 4.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 4);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 6);
        assert_eq!(pass_plan.quad_vertices.len(), 4);
        assert_eq!(pass_plan.quad_indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(pass_plan.quad_vertices[0].position, [4.0, 0.0]);
        assert_eq!(pass_plan.quad_vertices[2].position, [104.0, 50.0]);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_concave_filled_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.path_data = Some("M0 0 L100 0 L100 100 L50 50 L0 100 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 5);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 9);
        assert_eq!(pass_plan.quad_vertices.len(), 5);
        assert_eq!(pass_plan.quad_indices.len(), 9);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_compound_evenodd_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#22aa88".to_owned());
        path.path_fill_rule = ScenePathFillRule::Evenodd;
        path.path_data =
            Some("M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 16);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 24);
        assert_eq!(pass_plan.quad_vertices.len(), 16);
        assert_eq!(pass_plan.quad_indices.len(), 24);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_compound_nonzero_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#22aa88".to_owned());
        path.path_fill_rule = ScenePathFillRule::Nonzero;
        path.path_data =
            Some("M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 12);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 18);
        assert_eq!(pass_plan.quad_vertices.len(), 12);
        assert_eq!(pass_plan.quad_indices.len(), 18);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_cubic_curve_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.path_data = Some("M0 0 C25 80 75 -80 100 0 S175 80 200 0 L200 80 L0 80 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(
            pass_plan.quad_recording_steps[0].vertex_count,
            (SCENE_FULL_PATH_CURVE_SEGMENTS * 2 + 3) as u32
        );
        assert!(pass_plan.quad_recording_steps[0].index_count > 6);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
        assert_eq!(
            pass_plan.quad_vertices.last().map(|vertex| vertex.position),
            Some([0.0, 80.0])
        );
    }

    #[test]
    fn draw_pass_plan_records_quadratic_curve_path_stroke_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.stroke_color = Some("#ffffff".to_owned());
        path.stroke_width = Some(6.0);
        path.path_data = Some("M0 0 Q50 100 100 0 T200 0".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert!(pass_plan.quad_recording_steps[0].stroke_geometry);
        assert_eq!(
            pass_plan.quad_recording_steps[0].vertex_count,
            (SCENE_FULL_PATH_CURVE_SEGMENTS * 2 * 4) as u32
        );
        assert_eq!(
            pass_plan.quad_recording_steps[0].index_count,
            (SCENE_FULL_PATH_CURVE_SEGMENTS * 2 * 6) as u32
        );
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_arc_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#22aa88".to_owned());
        path.path_data = Some("M100 50 A50 50 0 1 1 0 50 A50 50 0 1 1 100 50 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(
            pass_plan.quad_recording_steps[0].vertex_count,
            (SCENE_FULL_PATH_ARC_SEGMENTS_PER_QUARTER * 4) as u32
        );
        assert!(pass_plan.quad_recording_steps[0].index_count > 6);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_text_as_solid_geometry() {
        let mut text = draw_op(0, NativeVulkanSceneDrawOpKind::Text);
        text.text = Some("A1".to_owned());
        text.color = Some("#ffffff".to_owned());
        text.font_size = Some(14.0);
        text.width = Some(80.0);
        text.text_align = Some(SceneTextAlign::Middle);
        text.transform.x = 10.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![text],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.recordable_op_count, 1);
        assert_eq!(pass_plan.recordable_quads[0].kind, "text");
        assert_eq!(pass_plan.recordable_quads[0].text.as_deref(), Some("A1"));
        assert_eq!(pass_plan.recordable_quads[0].font_size, Some(14.0));
        assert_eq!(
            pass_plan.recordable_quads[0].text_align,
            Some(SceneTextAlign::Middle)
        );
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "text");
        assert!(pass_plan.quad_recording_steps[0].vertex_count > 4);
        assert!(pass_plan.quad_recording_steps[0].index_count > 6);
        assert!(!pass_plan.quad_vertices.is_empty());
        assert!(!pass_plan.quad_indices.is_empty());
        assert_eq!(pass_plan.text_op_count, 1);
        assert!(!pass_plan.requires_text_geometry);
    }

    #[test]
    fn draw_pass_plan_records_filled_and_stroked_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.stroke_color = Some("#ffffff".to_owned());
        path.stroke_width = Some(4.0);
        path.path_data = Some("M0 0 L100 0 L100 50 L0 50 Z".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.recordable_op_count, 1);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 20);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 30);
        assert!(pass_plan.quad_recording_steps[0].fill_geometry);
        assert!(pass_plan.quad_recording_steps[0].stroke_geometry);
        assert_eq!(pass_plan.quad_vertices.len(), 20);
        assert_eq!(pass_plan.quad_indices.len(), 30);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
        assert_eq!(
            pass_plan.recordable_quads[0].fill_color.as_deref(),
            Some("#cc3300")
        );
        assert_eq!(
            pass_plan.recordable_quads[0].stroke_color.as_deref(),
            Some("#ffffff")
        );
        assert_eq!(pass_plan.recordable_quads[0].stroke_width, Some(4.0));
    }

    #[test]
    fn draw_pass_plan_records_stroke_only_path_as_solid_geometry() {
        let mut path = draw_op(0, NativeVulkanSceneDrawOpKind::Path);
        path.stroke_color = Some("#ffffff".to_owned());
        path.stroke_width = Some(6.0);
        path.path_data = Some("M0 0 L100 0 L100 50".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.recordable_op_count, 1);
        assert_eq!(pass_plan.recordable_quads[0].color, "#ffffff");
        assert_eq!(pass_plan.recordable_quads[0].fill_rgba, None);
        assert_eq!(
            pass_plan.recordable_quads[0].stroke_rgba,
            Some([1.0, 1.0, 1.0, 1.0])
        );
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert!(!pass_plan.quad_recording_steps[0].fill_geometry);
        assert!(pass_plan.quad_recording_steps[0].stroke_geometry);
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 8);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 12);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_records_stroke_only_rectangle_and_ellipse() {
        let mut rectangle = draw_op(0, NativeVulkanSceneDrawOpKind::Rectangle);
        rectangle.stroke_color = Some("#ffcc00".to_owned());
        rectangle.stroke_width = Some(4.0);
        rectangle.width = Some(100.0);
        rectangle.height = Some(50.0);
        let mut ellipse = draw_op(1, NativeVulkanSceneDrawOpKind::Ellipse);
        ellipse.stroke_color = Some("#00ccff".to_owned());
        ellipse.stroke_width = Some(8.0);
        ellipse.width = Some(80.0);
        ellipse.height = Some(40.0);
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![rectangle, ellipse],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.vector_shape_op_count, 2);
        assert_eq!(pass_plan.recordable_op_count, 2);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 2);
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "rectangle");
        assert!(!pass_plan.quad_recording_steps[0].fill_geometry);
        assert!(pass_plan.quad_recording_steps[0].stroke_geometry);
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 16);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 24);
        assert_eq!(pass_plan.quad_recording_steps[1].kind, "ellipse");
        assert!(!pass_plan.quad_recording_steps[1].fill_geometry);
        assert!(pass_plan.quad_recording_steps[1].stroke_geometry);
        assert_eq!(pass_plan.quad_recording_steps[1].vertex_count, 96);
        assert_eq!(pass_plan.quad_recording_steps[1].index_count, 288);
        assert_eq!(pass_plan.quad_vertices.len(), 112);
        assert_eq!(pass_plan.quad_indices.len(), 312);
    }

    #[test]
    fn draw_pass_plan_reports_solid_rectangle_quad_backend_ready() {
        let mut rectangle = draw_op(0, NativeVulkanSceneDrawOpKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.5;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.transform.x = 24.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            dynamic_topology_required: false,
            draw_ops: vec![rectangle],
            unsupported_layers: Vec::new(),
            runtime_display_available: false,
        };

        let pass_plan = native_vulkan_scene_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "solid-quad-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert!(pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 96);
        assert_eq!(pass_plan.quad_index_buffer_bytes, 24);
        let step = &pass_plan.quad_recording_steps[0];
        assert_eq!(step.layer_id, "layer-0");
        assert_eq!(step.kind, "rectangle");
        assert_eq!(
            super::blend::native_vulkan_scene_solid_quad_pipeline_label(step.blend),
            "solid-quad-alpha-blend"
        );
        assert_eq!(step.first_vertex, 0);
        assert_eq!(step.vertex_count, 4);
        assert_eq!(step.first_index, 0);
        assert_eq!(step.index_count, 6);
        assert_eq!(step.vertex_buffer_offset_bytes, 0);
        assert_eq!(step.vertex_buffer_size_bytes, 96);
        assert_eq!(step.index_buffer_offset_bytes, 0);
        assert_eq!(step.index_buffer_size_bytes, 24);
        assert_eq!(pass_plan.quad_indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(pass_plan.quad_vertices.len(), 4);
        assert_eq!(pass_plan.quad_vertices[0].position, [-296.0, -180.0]);
        assert_eq!(pass_plan.quad_vertices[1].position, [344.0, -180.0]);
        assert_eq!(pass_plan.quad_vertices[2].position, [-296.0, 180.0]);
        assert_eq!(pass_plan.quad_vertices[3].position, [344.0, 180.0]);
        assert_eq!(
            pass_plan.quad_vertices[0].rgba,
            [51.0 / 255.0, 102.0 / 255.0, 153.0 / 255.0, 0.5]
        );
    }
}

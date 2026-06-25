use std::path::PathBuf;

use crate::core::{FitMode, SceneLiteTransform};

use super::render_plan::{
    NativeVulkanSceneLiteDrawOp, NativeVulkanSceneLiteDrawOpKind, NativeVulkanSceneLiteDrawPlan,
};

const SCENE_LITE_SOLID_QUAD_VERTEX_COUNT: u32 = 4;
const SCENE_LITE_SOLID_QUAD_INDEX_COUNT: u32 = 6;
const SCENE_LITE_SOLID_QUAD_VERTEX_BYTES: u64 = 24;
const SCENE_LITE_SOLID_QUAD_INDEX_BYTES: u64 = 4;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT: u32 = 4;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT: u32 = 6;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES: u64 = 20;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES: u64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteRecordableQuad {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) kind: &'static str,
    pub(super) color: String,
    pub(super) rgba: [f32; 4],
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    pub(super) transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteQuadRecordingStep {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) kind: &'static str,
    pub(super) pipeline: &'static str,
    pub(super) first_vertex: u32,
    pub(super) vertex_count: u32,
    pub(super) first_index: u32,
    pub(super) index_count: u32,
    pub(super) vertex_buffer_offset_bytes: u64,
    pub(super) vertex_buffer_size_bytes: u64,
    pub(super) index_buffer_offset_bytes: u64,
    pub(super) index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteSampledImageQuad {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) source: PathBuf,
    pub(super) fit: FitMode,
    pub(super) opacity: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteSampledImageRecordingStep {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) source: PathBuf,
    pub(super) fit: FitMode,
    pub(super) pipeline: &'static str,
    pub(super) resource_index: u32,
    pub(super) first_vertex: u32,
    pub(super) vertex_count: u32,
    pub(super) first_index: u32,
    pub(super) index_count: u32,
    pub(super) vertex_buffer_offset_bytes: u64,
    pub(super) vertex_buffer_size_bytes: u64,
    pub(super) index_buffer_offset_bytes: u64,
    pub(super) index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NativeVulkanSceneLiteQuadVertex {
    pub(super) position: [f32; 2],
    pub(super) rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NativeVulkanSceneLiteSampledImageVertex {
    pub(super) position: [f32; 2],
    pub(super) uv: [f32; 2],
    pub(super) opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteDrawPassPlan {
    pub(super) plan_ready: bool,
    pub(super) backend_ready: bool,
    pub(super) backend_status: &'static str,
    pub(super) blocking_reason: Option<&'static str>,
    pub(super) recordable_op_count: usize,
    pub(super) recordable_quads: Vec<NativeVulkanSceneLiteRecordableQuad>,
    pub(super) quad_recording_ready: bool,
    pub(super) quad_recording_steps: Vec<NativeVulkanSceneLiteQuadRecordingStep>,
    pub(super) quad_vertices: Vec<NativeVulkanSceneLiteQuadVertex>,
    pub(super) quad_indices: Vec<u32>,
    pub(super) quad_vertex_buffer_bytes: u64,
    pub(super) quad_index_buffer_bytes: u64,
    pub(super) sampled_image_quads: Vec<NativeVulkanSceneLiteSampledImageQuad>,
    pub(super) sampled_image_recording_ready: bool,
    pub(super) sampled_image_recording_steps: Vec<NativeVulkanSceneLiteSampledImageRecordingStep>,
    pub(super) sampled_image_vertices: Vec<NativeVulkanSceneLiteSampledImageVertex>,
    pub(super) sampled_image_indices: Vec<u32>,
    pub(super) sampled_image_vertex_buffer_bytes: u64,
    pub(super) sampled_image_index_buffer_bytes: u64,
    pub(super) color_op_count: usize,
    pub(super) sampled_image_op_count: usize,
    pub(super) vector_shape_op_count: usize,
    pub(super) text_op_count: usize,
    pub(super) path_op_count: usize,
    pub(super) required_image_resources: Vec<PathBuf>,
    pub(super) requires_text_atlas: bool,
    pub(super) requires_path_tessellation: bool,
    pub(super) fast_clear_color: Option<String>,
}

pub(super) fn native_vulkan_scene_lite_draw_pass_plan(
    draw_plan: &NativeVulkanSceneLiteDrawPlan,
) -> NativeVulkanSceneLiteDrawPassPlan {
    let mut color_op_count = 0usize;
    let mut sampled_image_op_count = 0usize;
    let mut vector_shape_op_count = 0usize;
    let mut text_op_count = 0usize;
    let mut path_op_count = 0usize;
    let mut required_image_resources = Vec::new();

    for op in &draw_plan.draw_ops {
        match op.kind {
            NativeVulkanSceneLiteDrawOpKind::Image => {
                sampled_image_op_count = sampled_image_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    required_image_resources.push(source.clone());
                }
            }
            NativeVulkanSceneLiteDrawOpKind::ColorQuad => {
                color_op_count = color_op_count.saturating_add(1);
            }
            NativeVulkanSceneLiteDrawOpKind::Rectangle
            | NativeVulkanSceneLiteDrawOpKind::Ellipse => {
                vector_shape_op_count = vector_shape_op_count.saturating_add(1);
            }
            NativeVulkanSceneLiteDrawOpKind::Text => {
                text_op_count = text_op_count.saturating_add(1);
            }
            NativeVulkanSceneLiteDrawOpKind::Path => {
                path_op_count = path_op_count.saturating_add(1);
            }
        }
    }

    let fast_clear_color = native_vulkan_scene_lite_fast_clear_color(&draw_plan.draw_ops);
    let recordable_quads = draw_plan
        .draw_ops
        .iter()
        .filter_map(native_vulkan_scene_lite_recordable_quad)
        .collect::<Vec<_>>();
    let recordable_op_count = recordable_quads.len();
    let quad_recording_payload = native_vulkan_scene_lite_quad_recording_payload(&recordable_quads);
    let quad_recording_ready = !quad_recording_payload.steps.is_empty()
        && quad_recording_payload.steps.len() == draw_plan.draw_ops.len();
    let sampled_image_quads = draw_plan
        .draw_ops
        .iter()
        .filter_map(native_vulkan_scene_lite_sampled_image_quad)
        .collect::<Vec<_>>();
    let sampled_image_recording_payload =
        native_vulkan_scene_lite_sampled_image_recording_payload(&sampled_image_quads);
    let sampled_image_recording_ready = sampled_image_op_count > 0
        && sampled_image_recording_payload.steps.len() == sampled_image_op_count;
    let quad_vertex_buffer_bytes =
        native_vulkan_scene_lite_quad_vertex_buffer_bytes(quad_recording_payload.steps.len());
    let quad_index_buffer_bytes =
        native_vulkan_scene_lite_quad_index_buffer_bytes(quad_recording_payload.steps.len());
    let sampled_image_vertex_buffer_bytes =
        native_vulkan_scene_lite_sampled_image_vertex_buffer_bytes(
            sampled_image_recording_payload.steps.len(),
        );
    let sampled_image_index_buffer_bytes =
        native_vulkan_scene_lite_sampled_image_index_buffer_bytes(
            sampled_image_recording_payload.steps.len(),
        );
    let plan_ready = draw_plan.native_draw_ready();
    let sampled_image_recording_complete =
        sampled_image_recording_ready && sampled_image_op_count == draw_plan.draw_ops.len();
    let backend_ready = plan_ready
        && (fast_clear_color.is_some() || quad_recording_ready || sampled_image_recording_complete);
    let (backend_status, blocking_reason) = if !plan_ready {
        (
            "blocked-by-unsupported-scene-lite-layers",
            Some("unsupported-scene-lite-layers"),
        )
    } else if draw_plan.draw_ops.is_empty() {
        (
            "blocked-empty-scene-lite-draw-plan",
            Some("empty-draw-plan"),
        )
    } else if backend_ready {
        if fast_clear_color.is_some() {
            ("fast-clear-color-ready", None)
        } else if quad_recording_ready {
            ("solid-quad-recording-ready", None)
        } else {
            ("sampled-image-recording-ready", None)
        }
    } else if !quad_recording_payload.steps.is_empty() {
        (
            "partial-solid-quad-recording-ready",
            Some("non-quad-draw-ops-need-recording-backend"),
        )
    } else if !sampled_image_recording_payload.steps.is_empty() {
        (
            "partial-sampled-image-quad-payload-ready",
            Some("non-image-quad-draw-ops-need-recording-backend"),
        )
    } else if !recordable_quads.is_empty() {
        (
            "quad-payload-ready-recording-pending",
            Some("vulkan-quad-recording-not-implemented"),
        )
    } else {
        (
            "draw-pass-plan-ready-recording-pending",
            Some("vulkan-draw-recording-not-implemented"),
        )
    };

    NativeVulkanSceneLiteDrawPassPlan {
        plan_ready,
        backend_ready,
        backend_status,
        blocking_reason,
        recordable_op_count,
        recordable_quads,
        quad_recording_ready,
        quad_recording_steps: quad_recording_payload.steps,
        quad_vertices: quad_recording_payload.vertices,
        quad_indices: quad_recording_payload.indices,
        quad_vertex_buffer_bytes,
        quad_index_buffer_bytes,
        sampled_image_quads,
        sampled_image_recording_ready,
        sampled_image_recording_steps: sampled_image_recording_payload.steps,
        sampled_image_vertices: sampled_image_recording_payload.vertices,
        sampled_image_indices: sampled_image_recording_payload.indices,
        sampled_image_vertex_buffer_bytes,
        sampled_image_index_buffer_bytes,
        color_op_count,
        sampled_image_op_count,
        vector_shape_op_count,
        text_op_count,
        path_op_count,
        required_image_resources,
        requires_text_atlas: text_op_count > 0,
        requires_path_tessellation: path_op_count > 0,
        fast_clear_color,
    }
}

fn native_vulkan_scene_lite_fast_clear_color(
    draw_ops: &[NativeVulkanSceneLiteDrawOp],
) -> Option<String> {
    let [op] = draw_ops else {
        return None;
    };
    if op.kind != NativeVulkanSceneLiteDrawOpKind::ColorQuad
        || op.opacity < 1.0
        || op.transform != SceneLiteTransform::default()
    {
        return None;
    }
    op.color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned)
}

struct NativeVulkanSceneLiteQuadRecordingPayload {
    steps: Vec<NativeVulkanSceneLiteQuadRecordingStep>,
    vertices: Vec<NativeVulkanSceneLiteQuadVertex>,
    indices: Vec<u32>,
}

struct NativeVulkanSceneLiteSampledImageRecordingPayload {
    steps: Vec<NativeVulkanSceneLiteSampledImageRecordingStep>,
    vertices: Vec<NativeVulkanSceneLiteSampledImageVertex>,
    indices: Vec<u32>,
}

fn native_vulkan_scene_lite_quad_recording_payload(
    quads: &[NativeVulkanSceneLiteRecordableQuad],
) -> NativeVulkanSceneLiteQuadRecordingPayload {
    let mut steps = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for quad in quads
        .iter()
        .filter(|quad| native_vulkan_scene_lite_quad_has_recordable_geometry(quad))
    {
        let index = steps.len();
        if let Some(quad_vertices) = native_vulkan_scene_lite_quad_vertices(quad) {
            let first_vertex = (index as u32).saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_COUNT);
            let first_index = (index as u32).saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_COUNT);
            steps.push(NativeVulkanSceneLiteQuadRecordingStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                kind: quad.kind,
                pipeline: "solid-quad-alpha-blend",
                first_vertex,
                vertex_count: SCENE_LITE_SOLID_QUAD_VERTEX_COUNT,
                first_index,
                index_count: SCENE_LITE_SOLID_QUAD_INDEX_COUNT,
                vertex_buffer_offset_bytes: u64::from(first_vertex)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES),
                vertex_buffer_size_bytes: u64::from(SCENE_LITE_SOLID_QUAD_VERTEX_COUNT)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES),
                index_buffer_offset_bytes: u64::from(first_index)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES),
                index_buffer_size_bytes: u64::from(SCENE_LITE_SOLID_QUAD_INDEX_COUNT)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES),
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
    NativeVulkanSceneLiteQuadRecordingPayload {
        steps,
        vertices,
        indices,
    }
}

fn native_vulkan_scene_lite_sampled_image_recording_payload(
    quads: &[NativeVulkanSceneLiteSampledImageQuad],
) -> NativeVulkanSceneLiteSampledImageRecordingPayload {
    let mut steps = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for quad in quads
        .iter()
        .filter(|quad| native_vulkan_scene_lite_sampled_image_quad_has_recordable_geometry(quad))
    {
        let index = steps.len();
        if let Some(quad_vertices) = native_vulkan_scene_lite_sampled_image_vertices(quad) {
            let resource_index = index as u32;
            let first_vertex = (index as u32).saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT);
            let first_index = (index as u32).saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT);
            steps.push(NativeVulkanSceneLiteSampledImageRecordingStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                source: quad.source.clone(),
                fit: quad.fit,
                pipeline: "sampled-image-alpha-blend",
                resource_index,
                first_vertex,
                vertex_count: SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT,
                first_index,
                index_count: SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT,
                vertex_buffer_offset_bytes: u64::from(first_vertex)
                    .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES),
                vertex_buffer_size_bytes: u64::from(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT)
                    .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES),
                index_buffer_offset_bytes: u64::from(first_index)
                    .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES),
                index_buffer_size_bytes: u64::from(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT)
                    .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES),
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
    NativeVulkanSceneLiteSampledImageRecordingPayload {
        steps,
        vertices,
        indices,
    }
}

fn native_vulkan_scene_lite_quad_has_recordable_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> bool {
    matches!(quad.kind, "rectangle")
        && quad
            .width
            .is_some_and(|width| width.is_finite() && width > 0.0)
        && quad
            .height
            .is_some_and(|height| height.is_finite() && height > 0.0)
}

fn native_vulkan_scene_lite_sampled_image_quad_has_recordable_geometry(
    quad: &NativeVulkanSceneLiteSampledImageQuad,
) -> bool {
    quad.width.is_finite()
        && quad.width > 0.0
        && quad.height.is_finite()
        && quad.height > 0.0
        && quad.opacity.is_finite()
        && quad.opacity > 0.0
}

fn native_vulkan_scene_lite_quad_vertices(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<[NativeVulkanSceneLiteQuadVertex; 4]> {
    let points =
        native_vulkan_scene_lite_quad_positions(quad.width?, quad.height?, quad.transform)?;
    let mut vertices = [NativeVulkanSceneLiteQuadVertex {
        position: [0.0, 0.0],
        rgba: quad.rgba,
    }; 4];
    for (vertex, position) in vertices.iter_mut().zip(points) {
        vertex.position = position;
    }
    Some(vertices)
}

fn native_vulkan_scene_lite_sampled_image_vertices(
    quad: &NativeVulkanSceneLiteSampledImageQuad,
) -> Option<[NativeVulkanSceneLiteSampledImageVertex; 4]> {
    let points = native_vulkan_scene_lite_quad_positions(quad.width, quad.height, quad.transform)?;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    let mut vertices = [NativeVulkanSceneLiteSampledImageVertex {
        position: [0.0, 0.0],
        uv: [0.0, 0.0],
        opacity: quad.opacity.clamp(0.0, 1.0) as f32,
    }; 4];
    for ((vertex, position), uv) in vertices.iter_mut().zip(points).zip(uvs) {
        vertex.position = position;
        vertex.uv = uv;
    }
    Some(vertices)
}

fn native_vulkan_scene_lite_quad_positions(
    width: f64,
    height: f64,
    transform: SceneLiteTransform,
) -> Option<[[f32; 2]; 4]> {
    let left = -transform.anchor_x * width;
    let top = -transform.anchor_y * height;
    let right = left + width;
    let bottom = top + height;
    let rotation = transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let points = [(left, top), (right, top), (left, bottom), (right, bottom)];
    let mut positions = [[0.0, 0.0]; 4];
    for (position, (x, y)) in positions.iter_mut().zip(points) {
        let scaled_x = x * transform.scale_x;
        let scaled_y = y * transform.scale_y;
        let scene_x = scaled_x.mul_add(cos, -scaled_y * sin) + transform.x;
        let scene_y = scaled_x.mul_add(sin, scaled_y * cos) + transform.y;
        if !scene_x.is_finite() || !scene_y.is_finite() {
            return None;
        }
        *position = [scene_x as f32, scene_y as f32];
    }
    Some(positions)
}

fn native_vulkan_scene_lite_quad_vertex_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SOLID_QUAD_VERTEX_COUNT))
        .saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES)
}

fn native_vulkan_scene_lite_quad_index_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SOLID_QUAD_INDEX_COUNT))
        .saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES)
}

fn native_vulkan_scene_lite_sampled_image_vertex_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT))
        .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES)
}

fn native_vulkan_scene_lite_sampled_image_index_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT))
        .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES)
}

fn native_vulkan_scene_lite_recordable_quad(
    op: &NativeVulkanSceneLiteDrawOp,
) -> Option<NativeVulkanSceneLiteRecordableQuad> {
    match op.kind {
        NativeVulkanSceneLiteDrawOpKind::ColorQuad => {
            native_vulkan_scene_lite_recordable_quad_from_op(op, "color-quad")
        }
        NativeVulkanSceneLiteDrawOpKind::Rectangle
            if !native_vulkan_scene_lite_rectangle_needs_shape_pipeline(op) =>
        {
            native_vulkan_scene_lite_recordable_quad_from_op(op, "rectangle")
        }
        _ => None,
    }
}

fn native_vulkan_scene_lite_sampled_image_quad(
    op: &NativeVulkanSceneLiteDrawOp,
) -> Option<NativeVulkanSceneLiteSampledImageQuad> {
    if op.kind != NativeVulkanSceneLiteDrawOpKind::Image {
        return None;
    }
    Some(NativeVulkanSceneLiteSampledImageQuad {
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

fn native_vulkan_scene_lite_recordable_quad_from_op(
    op: &NativeVulkanSceneLiteDrawOp,
    kind: &'static str,
) -> Option<NativeVulkanSceneLiteRecordableQuad> {
    let color = op
        .color
        .as_deref()
        .filter(|color| !color.is_empty())?
        .to_owned();
    let rgba = native_vulkan_scene_lite_rgba_from_hex(&color, op.opacity)?;
    Some(NativeVulkanSceneLiteRecordableQuad {
        layer_index: op.layer_index,
        layer_id: op.layer_id.clone(),
        kind,
        color,
        rgba,
        width: op.width,
        height: op.height,
        transform: op.transform,
    })
}

fn native_vulkan_scene_lite_rectangle_needs_shape_pipeline(
    op: &NativeVulkanSceneLiteDrawOp,
) -> bool {
    op.stroke_color
        .as_deref()
        .is_some_and(|color| !color.is_empty())
        && op.stroke_width.unwrap_or(1.0) > 0.0
        || op.corner_radius.unwrap_or(0.0) > 0.0
}

fn native_vulkan_scene_lite_rgba_from_hex(color: &str, opacity: f64) -> Option<[f32; 4]> {
    let hex = color.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some([r, g, b, opacity.clamp(0.0, 1.0) as f32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FitMode;

    fn draw_op(
        layer_index: usize,
        kind: NativeVulkanSceneLiteDrawOpKind,
    ) -> NativeVulkanSceneLiteDrawOp {
        NativeVulkanSceneLiteDrawOp {
            layer_index,
            layer_id: format!("layer-{layer_index}"),
            kind,
            opacity: 1.0,
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
            transform: SceneLiteTransform::default(),
        }
    }

    #[test]
    fn draw_pass_plan_reports_fast_clear_color_ready() {
        let mut color = draw_op(0, NativeVulkanSceneLiteDrawOpKind::ColorQuad);
        color.color = Some("#102030".to_owned());
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![color],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

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
    }

    #[test]
    fn draw_pass_plan_reports_resource_buckets_and_pending_backend() {
        let mut image = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));
        let text = draw_op(1, NativeVulkanSceneLiteDrawOpKind::Text);
        let path = draw_op(2, NativeVulkanSceneLiteDrawOpKind::Path);
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image, text, path],
            unsupported_layers: Vec::new(),
            fallback_display_available: true,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(!pass_plan.backend_ready);
        assert_eq!(
            pass_plan.blocking_reason,
            Some("vulkan-draw-recording-not-implemented")
        );
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.text_op_count, 1);
        assert_eq!(pass_plan.path_op_count, 1);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/hero.png")]
        );
        assert!(pass_plan.requires_text_atlas);
        assert!(pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_reports_sampled_image_quad_payload() {
        let mut image = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));
        image.fit = FitMode::Contain;
        image.opacity = 0.75;
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.transform.x = 16.0;
        image.transform.y = 8.0;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(pass_plan.backend_status, "sampled-image-recording-ready");
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 1);
        assert!(pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 1);
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 80);
        assert_eq!(pass_plan.sampled_image_index_buffer_bytes, 24);
        assert_eq!(pass_plan.sampled_image_indices, vec![0, 1, 2, 2, 1, 3]);
        let step = &pass_plan.sampled_image_recording_steps[0];
        assert_eq!(step.pipeline, "sampled-image-alpha-blend");
        assert_eq!(step.source, PathBuf::from("/tmp/hero.png"));
        assert_eq!(step.fit, FitMode::Contain);
        assert_eq!(step.resource_index, 0);
        assert_eq!(step.vertex_count, 4);
        assert_eq!(step.index_count, 6);
        assert_eq!(pass_plan.sampled_image_vertices.len(), 4);
        assert_eq!(
            pass_plan.sampled_image_vertices[0].position,
            [-144.0, -82.0]
        );
        assert_eq!(pass_plan.sampled_image_vertices[3].position, [176.0, 98.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].uv, [0.0, 0.0]);
        assert_eq!(pass_plan.sampled_image_vertices[3].uv, [1.0, 1.0]);
        assert_eq!(pass_plan.sampled_image_vertices[0].opacity, 0.75);
    }

    #[test]
    fn draw_pass_plan_reports_recordable_rectangle_quad_payload() {
        let mut rectangle = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.5;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.transform.x = 24.0;
        let mut rounded = draw_op(1, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rounded.color = Some("#ffffff".to_owned());
        rounded.corner_radius = Some(8.0);
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![rectangle, rounded],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(!pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "partial-solid-quad-recording-ready"
        );
        assert_eq!(
            pass_plan.blocking_reason,
            Some("non-quad-draw-ops-need-recording-backend")
        );
        assert_eq!(pass_plan.vector_shape_op_count, 2);
        assert_eq!(pass_plan.recordable_op_count, 1);
        assert_eq!(pass_plan.recordable_quads.len(), 1);
        assert!(!pass_plan.quad_recording_ready);
        assert_eq!(pass_plan.quad_recording_steps.len(), 1);
        assert_eq!(pass_plan.quad_vertex_buffer_bytes, 96);
        assert_eq!(pass_plan.quad_index_buffer_bytes, 24);
        assert_eq!(
            pass_plan.quad_recording_steps[0].pipeline,
            "solid-quad-alpha-blend"
        );
        assert_eq!(pass_plan.quad_vertices.len(), 4);
        assert_eq!(pass_plan.quad_indices, vec![0, 1, 2, 2, 1, 3]);
        let quad = &pass_plan.recordable_quads[0];
        assert_eq!(quad.kind, "rectangle");
        assert_eq!(quad.color, "#336699");
        assert_eq!(quad.rgba, [51.0 / 255.0, 102.0 / 255.0, 153.0 / 255.0, 0.5]);
        assert_eq!(quad.width, Some(640.0));
        assert_eq!(quad.height, Some(360.0));
        assert_eq!(quad.transform.x, 24.0);
    }

    #[test]
    fn draw_pass_plan_reports_solid_rectangle_quad_backend_ready() {
        let mut rectangle = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.5;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.transform.x = 24.0;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![rectangle],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

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
        assert_eq!(step.pipeline, "solid-quad-alpha-blend");
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

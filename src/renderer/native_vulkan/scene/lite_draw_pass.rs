use std::path::PathBuf;

use crate::core::{FitMode, SceneLiteTransform};

use super::super::present::render_plan::{
    NativeVulkanSceneLiteDrawOp, NativeVulkanSceneLiteDrawOpKind, NativeVulkanSceneLiteDrawPlan,
};

const SCENE_LITE_SOLID_QUAD_VERTEX_BYTES: u64 = 24;
const SCENE_LITE_SOLID_QUAD_INDEX_BYTES: u64 = 4;
const SCENE_LITE_ELLIPSE_SEGMENTS: usize = 48;
const SCENE_LITE_ROUNDED_RECT_CORNER_SEGMENTS: usize = 8;
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
    pub(super) corner_radius: Option<f64>,
    pub(super) path_data: Option<String>,
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
    pub(super) sampled_image_full_extent_fallback_ready: bool,
    pub(super) sampled_image_recording_steps: Vec<NativeVulkanSceneLiteSampledImageRecordingStep>,
    pub(super) sampled_image_vertices: Vec<NativeVulkanSceneLiteSampledImageVertex>,
    pub(super) sampled_image_indices: Vec<u32>,
    pub(super) sampled_image_vertex_buffer_bytes: u64,
    pub(super) sampled_image_index_buffer_bytes: u64,
    pub(super) clear_background_op_count: usize,
    pub(super) background_clear_color: Option<String>,
    pub(super) color_op_count: usize,
    pub(super) sampled_image_op_count: usize,
    pub(super) video_op_count: usize,
    pub(super) vector_shape_op_count: usize,
    pub(super) text_op_count: usize,
    pub(super) path_op_count: usize,
    pub(super) required_image_resources: Vec<PathBuf>,
    pub(super) required_video_resources: Vec<PathBuf>,
    pub(super) requires_text_atlas: bool,
    pub(super) requires_path_tessellation: bool,
    pub(super) requires_video_decode: bool,
    pub(super) fast_clear_color: Option<String>,
}

pub(super) fn native_vulkan_scene_lite_draw_pass_plan(
    draw_plan: &NativeVulkanSceneLiteDrawPlan,
) -> NativeVulkanSceneLiteDrawPassPlan {
    let mut color_op_count = 0usize;
    let mut sampled_image_op_count = 0usize;
    let mut video_op_count = 0usize;
    let mut vector_shape_op_count = 0usize;
    let mut text_op_count = 0usize;
    let mut path_op_count = 0usize;
    let mut required_image_resources = Vec::new();
    let mut required_video_resources = Vec::new();

    for op in &draw_plan.draw_ops {
        match op.kind {
            NativeVulkanSceneLiteDrawOpKind::Image => {
                sampled_image_op_count = sampled_image_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    required_image_resources.push(source.clone());
                }
            }
            NativeVulkanSceneLiteDrawOpKind::Video => {
                video_op_count = video_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    required_video_resources.push(source.clone());
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
    let background_clear_color =
        native_vulkan_scene_lite_background_clear_color(&draw_plan.draw_ops);
    let clear_background_op_count = usize::from(background_clear_color.is_some());
    let recordable_quads = draw_plan
        .draw_ops
        .iter()
        .filter_map(native_vulkan_scene_lite_recordable_quad)
        .collect::<Vec<_>>();
    let recordable_op_count = recordable_quads.len();
    let quad_recording_payload = native_vulkan_scene_lite_quad_recording_payload(&recordable_quads);
    let recorded_path_geometry_count = quad_recording_payload
        .steps
        .iter()
        .filter(|step| step.kind == "path")
        .count();
    let quad_recording_ready = !quad_recording_payload.steps.is_empty()
        && quad_recording_payload
            .steps
            .len()
            .saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let sampled_image_quads = draw_plan
        .draw_ops
        .iter()
        .filter_map(native_vulkan_scene_lite_sampled_image_quad)
        .collect::<Vec<_>>();
    let sampled_image_recording_payload =
        native_vulkan_scene_lite_sampled_image_recording_payload(&sampled_image_quads);
    let sampled_image_recording_ready = sampled_image_op_count > 0
        && sampled_image_recording_payload.steps.len() == sampled_image_op_count;
    let full_extent_sampled_image_op_count =
        native_vulkan_scene_lite_full_extent_sampled_image_op_count(&draw_plan.draw_ops);
    let sampled_image_full_extent_fallback_ready =
        full_extent_sampled_image_op_count == 1 && sampled_image_op_count == 1;
    let mixed_quad_sampled_image_recording_ready = !quad_recording_payload.steps.is_empty()
        && sampled_image_recording_ready
        && quad_recording_payload
            .steps
            .len()
            .saturating_add(sampled_image_recording_payload.steps.len())
            .saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let mixed_quad_sampled_image_full_extent_fallback_ready =
        !quad_recording_payload.steps.is_empty()
            && full_extent_sampled_image_op_count > 0
            && full_extent_sampled_image_op_count == 1
            && full_extent_sampled_image_op_count == sampled_image_op_count
            && quad_recording_payload
                .steps
                .len()
                .saturating_add(sampled_image_op_count)
                .saturating_add(clear_background_op_count)
                == draw_plan.draw_ops.len();
    let quad_vertex_buffer_bytes =
        native_vulkan_scene_lite_solid_vertex_buffer_bytes(quad_recording_payload.vertices.len());
    let quad_index_buffer_bytes =
        native_vulkan_scene_lite_solid_index_buffer_bytes(quad_recording_payload.indices.len());
    let sampled_image_vertex_buffer_bytes =
        native_vulkan_scene_lite_sampled_image_vertex_buffer_bytes(
            sampled_image_recording_payload.steps.len(),
        );
    let sampled_image_index_buffer_bytes =
        native_vulkan_scene_lite_sampled_image_index_buffer_bytes(
            sampled_image_recording_payload.steps.len(),
        );
    let plan_ready = draw_plan.native_draw_ready();
    let sampled_image_recording_complete = sampled_image_recording_ready
        && sampled_image_op_count.saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let sampled_image_full_extent_fallback_backend_ready = sampled_image_full_extent_fallback_ready
        && sampled_image_op_count.saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let backend_ready = plan_ready
        && (fast_clear_color.is_some()
            || quad_recording_ready
            || sampled_image_recording_complete
            || sampled_image_full_extent_fallback_backend_ready
            || mixed_quad_sampled_image_full_extent_fallback_ready
            || mixed_quad_sampled_image_recording_ready);
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
        } else if quad_recording_ready && clear_background_op_count > 0 {
            ("clear-background-solid-quad-recording-ready", None)
        } else if quad_recording_ready {
            ("solid-quad-recording-ready", None)
        } else if mixed_quad_sampled_image_recording_ready && clear_background_op_count > 0 {
            (
                "clear-background-mixed-quad-sampled-image-recording-ready",
                None,
            )
        } else if mixed_quad_sampled_image_recording_ready {
            ("mixed-quad-sampled-image-recording-ready", None)
        } else if mixed_quad_sampled_image_full_extent_fallback_ready
            && clear_background_op_count > 0
        {
            (
                "clear-background-mixed-quad-sampled-image-full-extent-fallback-ready",
                None,
            )
        } else if mixed_quad_sampled_image_full_extent_fallback_ready {
            ("mixed-quad-sampled-image-full-extent-fallback-ready", None)
        } else if sampled_image_full_extent_fallback_backend_ready && clear_background_op_count > 0
        {
            (
                "clear-background-sampled-image-full-extent-fallback-ready",
                None,
            )
        } else if sampled_image_full_extent_fallback_backend_ready {
            ("sampled-image-full-extent-fallback-ready", None)
        } else if sampled_image_recording_complete && clear_background_op_count > 0 {
            ("clear-background-sampled-image-recording-ready", None)
        } else {
            ("sampled-image-recording-ready", None)
        }
    } else if video_op_count > 0 {
        (
            "video-layer-vulkan-video-scene-bridge-pending",
            Some("video-layer-needs-vulkan-video-scene-bridge"),
        )
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
        sampled_image_full_extent_fallback_ready,
        sampled_image_recording_steps: sampled_image_recording_payload.steps,
        sampled_image_vertices: sampled_image_recording_payload.vertices,
        sampled_image_indices: sampled_image_recording_payload.indices,
        sampled_image_vertex_buffer_bytes,
        sampled_image_index_buffer_bytes,
        clear_background_op_count,
        background_clear_color,
        color_op_count,
        sampled_image_op_count,
        video_op_count,
        vector_shape_op_count,
        text_op_count,
        path_op_count,
        required_image_resources,
        required_video_resources,
        requires_text_atlas: text_op_count > 0,
        requires_path_tessellation: path_op_count > recorded_path_geometry_count,
        requires_video_decode: video_op_count > 0,
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

fn native_vulkan_scene_lite_background_clear_color(
    draw_ops: &[NativeVulkanSceneLiteDrawOp],
) -> Option<String> {
    let [op, ..] = draw_ops else {
        return None;
    };
    if draw_ops.len() <= 1
        || op.kind != NativeVulkanSceneLiteDrawOpKind::ColorQuad
        || op.opacity < 1.0
        || op.width.is_some()
        || op.height.is_some()
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
        .filter(|quad| native_vulkan_scene_lite_solid_has_recordable_geometry(quad))
    {
        if let Some((solid_vertices, solid_indices)) = native_vulkan_scene_lite_solid_geometry(quad)
        {
            let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
            let first_index = indices.len().min(u32::MAX as usize) as u32;
            let vertex_count = solid_vertices.len().min(u32::MAX as usize) as u32;
            let index_count = solid_indices.len().min(u32::MAX as usize) as u32;
            steps.push(NativeVulkanSceneLiteQuadRecordingStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                kind: quad.kind,
                pipeline: "solid-quad-alpha-blend",
                first_vertex,
                vertex_count,
                first_index,
                index_count,
                vertex_buffer_offset_bytes: u64::from(first_vertex)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES),
                vertex_buffer_size_bytes: u64::from(vertex_count)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES),
                index_buffer_offset_bytes: u64::from(first_index)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES),
                index_buffer_size_bytes: u64::from(index_count)
                    .saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES),
            });
            vertices.extend(solid_vertices);
            indices.extend(
                solid_indices
                    .into_iter()
                    .map(|index| first_vertex.saturating_add(index)),
            );
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

fn native_vulkan_scene_lite_solid_has_recordable_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> bool {
    match quad.kind {
        "rectangle" | "rounded-rectangle" | "ellipse" => {
            quad.width
                .is_some_and(|width| width.is_finite() && width > 0.0)
                && quad
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
        }
        "path" => quad
            .path_data
            .as_deref()
            .is_some_and(|path| !path.is_empty()),
        _ => false,
    }
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

fn native_vulkan_scene_lite_solid_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneLiteQuadVertex>, Vec<u32>)> {
    match quad.kind {
        "rectangle" => native_vulkan_scene_lite_rectangle_geometry(quad),
        "rounded-rectangle" => native_vulkan_scene_lite_rounded_rectangle_geometry(quad),
        "ellipse" => native_vulkan_scene_lite_ellipse_geometry(quad),
        "path" => native_vulkan_scene_lite_path_geometry(quad),
        _ => None,
    }
}

fn native_vulkan_scene_lite_rectangle_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneLiteQuadVertex>, Vec<u32>)> {
    let points =
        native_vulkan_scene_lite_quad_positions(quad.width?, quad.height?, quad.transform)?;
    let vertices = points
        .into_iter()
        .map(|position| NativeVulkanSceneLiteQuadVertex {
            position,
            rgba: quad.rgba,
        })
        .collect();
    Some((vertices, vec![0, 1, 2, 2, 1, 3]))
}

fn native_vulkan_scene_lite_rounded_rectangle_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneLiteQuadVertex>, Vec<u32>)> {
    let width = quad.width?;
    let height = quad.height?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let radius = quad.corner_radius?.clamp(0.0, width.min(height) * 0.5);
    if !radius.is_finite() || radius <= 0.0 {
        return native_vulkan_scene_lite_rectangle_geometry(quad);
    }

    let left = -quad.transform.anchor_x * width;
    let top = -quad.transform.anchor_y * height;
    let right = left + width;
    let bottom = top + height;
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

    let mut outline = Vec::with_capacity((SCENE_LITE_ROUNDED_RECT_CORNER_SEGMENTS + 1) * 4);
    for (center_x, center_y, start_angle, end_angle) in corners {
        for segment in 0..=SCENE_LITE_ROUNDED_RECT_CORNER_SEGMENTS {
            let t = segment as f64 / SCENE_LITE_ROUNDED_RECT_CORNER_SEGMENTS as f64;
            let angle = start_angle + (end_angle - start_angle) * t;
            outline.push([
                center_x + angle.cos() * radius,
                center_y + angle.sin() * radius,
            ]);
        }
    }

    let mut vertices = Vec::with_capacity(outline.len() + 1);
    vertices.push(NativeVulkanSceneLiteQuadVertex {
        position: native_vulkan_scene_lite_transform_point(
            (left + right) * 0.5,
            (top + bottom) * 0.5,
            quad.transform,
        )?,
        rgba: quad.rgba,
    });
    vertices.extend(
        outline
            .into_iter()
            .map(|[x, y]| {
                Some(NativeVulkanSceneLiteQuadVertex {
                    position: native_vulkan_scene_lite_transform_point(x, y, quad.transform)?,
                    rgba: quad.rgba,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    );

    let outline_count = vertices.len().saturating_sub(1);
    let mut indices = Vec::with_capacity(outline_count * 3);
    for index in 0..outline_count {
        let current = index as u32 + 1;
        let next = if index + 1 == outline_count {
            1
        } else {
            current + 1
        };
        indices.extend_from_slice(&[0, current, next]);
    }
    Some((vertices, indices))
}

fn native_vulkan_scene_lite_ellipse_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneLiteQuadVertex>, Vec<u32>)> {
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
    let mut vertices = Vec::with_capacity(SCENE_LITE_ELLIPSE_SEGMENTS + 1);
    vertices.push(NativeVulkanSceneLiteQuadVertex {
        position: native_vulkan_scene_lite_transform_point(center_x, center_y, quad.transform)?,
        rgba: quad.rgba,
    });
    for segment in 0..SCENE_LITE_ELLIPSE_SEGMENTS {
        let theta = (segment as f64) * std::f64::consts::TAU / (SCENE_LITE_ELLIPSE_SEGMENTS as f64);
        let x = center_x + theta.cos() * radius_x;
        let y = center_y + theta.sin() * radius_y;
        vertices.push(NativeVulkanSceneLiteQuadVertex {
            position: native_vulkan_scene_lite_transform_point(x, y, quad.transform)?,
            rgba: quad.rgba,
        });
    }

    let mut indices = Vec::with_capacity(SCENE_LITE_ELLIPSE_SEGMENTS * 3);
    for segment in 0..SCENE_LITE_ELLIPSE_SEGMENTS {
        let current = 1 + segment as u32;
        let next = if segment + 1 == SCENE_LITE_ELLIPSE_SEGMENTS {
            1
        } else {
            current + 1
        };
        indices.extend_from_slice(&[0, current, next]);
    }
    Some((vertices, indices))
}

fn native_vulkan_scene_lite_path_geometry(
    quad: &NativeVulkanSceneLiteRecordableQuad,
) -> Option<(Vec<NativeVulkanSceneLiteQuadVertex>, Vec<u32>)> {
    let points = native_vulkan_scene_lite_simple_path_points(quad.path_data.as_deref()?)?;
    if points.len() < 3 {
        return None;
    }
    let indices = if native_vulkan_scene_lite_polygon_is_convex(&points) {
        let mut indices = Vec::with_capacity((points.len().saturating_sub(2)) * 3);
        for index in 1..points.len().saturating_sub(1) {
            indices.extend_from_slice(&[0, index as u32, index as u32 + 1]);
        }
        indices
    } else {
        native_vulkan_scene_lite_triangulate_simple_polygon(&points)?
    };
    let vertices = points
        .into_iter()
        .map(|[x, y]| {
            Some(NativeVulkanSceneLiteQuadVertex {
                position: native_vulkan_scene_lite_transform_point(x, y, quad.transform)?,
                rgba: quad.rgba,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((vertices, indices))
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
        *position =
            native_vulkan_scene_lite_transform_point_with_rotation(x, y, transform, cos, sin)?;
    }
    Some(positions)
}

fn native_vulkan_scene_lite_transform_point(
    x: f64,
    y: f64,
    transform: SceneLiteTransform,
) -> Option<[f32; 2]> {
    let rotation = transform.rotation_deg.to_radians();
    native_vulkan_scene_lite_transform_point_with_rotation(
        x,
        y,
        transform,
        rotation.cos(),
        rotation.sin(),
    )
}

fn native_vulkan_scene_lite_transform_point_with_rotation(
    x: f64,
    y: f64,
    transform: SceneLiteTransform,
    cos: f64,
    sin: f64,
) -> Option<[f32; 2]> {
    let scaled_x = x * transform.scale_x;
    let scaled_y = y * transform.scale_y;
    let scene_x = scaled_x.mul_add(cos, -scaled_y * sin) + transform.x;
    let scene_y = scaled_x.mul_add(sin, scaled_y * cos) + transform.y;
    if !scene_x.is_finite() || !scene_y.is_finite() {
        return None;
    }
    Some([scene_x as f32, scene_y as f32])
}

fn native_vulkan_scene_lite_solid_vertex_buffer_bytes(vertex_count: usize) -> u64 {
    (vertex_count as u64).saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES)
}

fn native_vulkan_scene_lite_solid_index_buffer_bytes(index_count: usize) -> u64 {
    (index_count as u64).saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES)
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
            if !native_vulkan_scene_lite_rectangle_needs_stroke_geometry(op) =>
        {
            native_vulkan_scene_lite_recordable_quad_from_op(
                op,
                native_vulkan_scene_lite_rectangle_recordable_kind(op),
            )
        }
        NativeVulkanSceneLiteDrawOpKind::Ellipse => {
            native_vulkan_scene_lite_recordable_quad_from_op(op, "ellipse")
        }
        NativeVulkanSceneLiteDrawOpKind::Path
            if op.stroke_color.as_deref().is_none_or(str::is_empty) =>
        {
            native_vulkan_scene_lite_recordable_quad_from_op(op, "path")
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

fn native_vulkan_scene_lite_full_extent_sampled_image_op_count(
    draw_ops: &[NativeVulkanSceneLiteDrawOp],
) -> usize {
    draw_ops
        .iter()
        .filter(|op| native_vulkan_scene_lite_full_extent_sampled_image_op_ready(op))
        .count()
}

fn native_vulkan_scene_lite_full_extent_sampled_image_op_ready(
    op: &NativeVulkanSceneLiteDrawOp,
) -> bool {
    op.kind == NativeVulkanSceneLiteDrawOpKind::Image
        && op.source.is_some()
        && op.opacity == 1.0
        && op.width.is_none()
        && op.height.is_none()
        && op.transform == SceneLiteTransform::default()
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
        corner_radius: op.corner_radius,
        path_data: op.path_data.clone(),
        transform: op.transform,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeVulkanSceneLitePathToken {
    Command(char),
    Number(f64),
}

fn native_vulkan_scene_lite_simple_path_points(path: &str) -> Option<Vec<[f64; 2]>> {
    let tokens = native_vulkan_scene_lite_path_tokens(path)?;
    let mut index = 0usize;
    let mut command = None::<char>;
    let mut points = Vec::new();
    let mut current = [0.0, 0.0];
    let mut start = [0.0, 0.0];
    let mut started = false;

    while index < tokens.len() {
        if let NativeVulkanSceneLitePathToken::Command(value) = tokens[index] {
            command = Some(value);
            index += 1;
        }
        let command = command?;
        match command {
            'M' | 'm' => {
                let relative = command == 'm';
                let mut first = true;
                while let Some((x, y, next_index)) =
                    native_vulkan_scene_lite_take_path_pair(&tokens, index)
                {
                    index = next_index;
                    let point = if relative {
                        [current[0] + x, current[1] + y]
                    } else {
                        [x, y]
                    };
                    current = point;
                    if first {
                        if started && points.len() >= 3 {
                            return None;
                        }
                        start = point;
                        started = true;
                        first = false;
                    }
                    points.push(point);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanSceneLitePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'L' | 'l' => {
                let relative = command == 'l';
                while let Some((x, y, next_index)) =
                    native_vulkan_scene_lite_take_path_pair(&tokens, index)
                {
                    index = next_index;
                    current = if relative {
                        [current[0] + x, current[1] + y]
                    } else {
                        [x, y]
                    };
                    points.push(current);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanSceneLitePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'H' | 'h' => {
                let relative = command == 'h';
                while let Some((x, next_index)) =
                    native_vulkan_scene_lite_take_path_number(&tokens, index)
                {
                    index = next_index;
                    current[0] = if relative { current[0] + x } else { x };
                    points.push(current);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanSceneLitePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'V' | 'v' => {
                let relative = command == 'v';
                while let Some((y, next_index)) =
                    native_vulkan_scene_lite_take_path_number(&tokens, index)
                {
                    index = next_index;
                    current[1] = if relative { current[1] + y } else { y };
                    points.push(current);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanSceneLitePathToken::Command(_))
                    {
                        break;
                    }
                }
            }
            'Z' | 'z' => {
                current = start;
                if points.last().copied() == Some(start) {
                    let _ = points.pop();
                }
            }
            _ => return None,
        }
    }

    points.dedup_by(|left, right| {
        (left[0] - right[0]).abs() < f64::EPSILON && (left[1] - right[1]).abs() < f64::EPSILON
    });
    Some(points)
}

fn native_vulkan_scene_lite_path_tokens(path: &str) -> Option<Vec<NativeVulkanSceneLitePathToken>> {
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
            'M' | 'm' | 'L' | 'l' | 'H' | 'h' | 'V' | 'v' | 'Z' | 'z'
        ) {
            tokens.push(NativeVulkanSceneLitePathToken::Command(ch));
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
            tokens.push(NativeVulkanSceneLitePathToken::Number(value));
            continue;
        }
        return None;
    }
    Some(tokens)
}

fn native_vulkan_scene_lite_take_path_number(
    tokens: &[NativeVulkanSceneLitePathToken],
    index: usize,
) -> Option<(f64, usize)> {
    match tokens.get(index)? {
        NativeVulkanSceneLitePathToken::Number(value) => Some((*value, index + 1)),
        NativeVulkanSceneLitePathToken::Command(_) => None,
    }
}

fn native_vulkan_scene_lite_take_path_pair(
    tokens: &[NativeVulkanSceneLitePathToken],
    index: usize,
) -> Option<(f64, f64, usize)> {
    let (x, index) = native_vulkan_scene_lite_take_path_number(tokens, index)?;
    let (y, index) = native_vulkan_scene_lite_take_path_number(tokens, index)?;
    Some((x, y, index))
}

fn native_vulkan_scene_lite_polygon_is_convex(points: &[[f64; 2]]) -> bool {
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

fn native_vulkan_scene_lite_triangulate_simple_polygon(points: &[[f64; 2]]) -> Option<Vec<u32>> {
    if points.len() < 3 || points.len() > u32::MAX as usize {
        return None;
    }
    let area = native_vulkan_scene_lite_polygon_signed_area(points);
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
            if !native_vulkan_scene_lite_triangle_is_counter_clockwise(
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
                    && native_vulkan_scene_lite_point_in_triangle(
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

fn native_vulkan_scene_lite_polygon_signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        area += a[0].mul_add(b[1], -b[0] * a[1]);
    }
    area * 0.5
}

fn native_vulkan_scene_lite_triangle_is_counter_clockwise(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ac = [c[0] - a[0], c[1] - a[1]];
    ab[0].mul_add(ac[1], -ab[1] * ac[0]) > f64::EPSILON
}

fn native_vulkan_scene_lite_point_in_triangle(
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

fn native_vulkan_scene_lite_rectangle_recordable_kind(
    op: &NativeVulkanSceneLiteDrawOp,
) -> &'static str {
    if op
        .corner_radius
        .is_some_and(|radius| radius.is_finite() && radius > 0.0)
    {
        "rounded-rectangle"
    } else {
        "rectangle"
    }
}

fn native_vulkan_scene_lite_rectangle_needs_stroke_geometry(
    op: &NativeVulkanSceneLiteDrawOp,
) -> bool {
    op.stroke_color
        .as_deref()
        .is_some_and(|color| !color.is_empty())
        && op.stroke_width.unwrap_or(1.0) > 0.0
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
        assert_eq!(pass_plan.video_op_count, 0);
        assert_eq!(pass_plan.text_op_count, 1);
        assert_eq!(pass_plan.path_op_count, 1);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/hero.png")]
        );
        assert!(pass_plan.requires_text_atlas);
        assert!(pass_plan.requires_path_tessellation);
        assert!(!pass_plan.requires_video_decode);
    }

    #[test]
    fn draw_pass_plan_reports_video_layer_bridge_pending() {
        let mut video = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        video.fit = FitMode::Cover;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![video],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(!pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "video-layer-vulkan-video-scene-bridge-pending"
        );
        assert_eq!(
            pass_plan.blocking_reason,
            Some("video-layer-needs-vulkan-video-scene-bridge")
        );
        assert_eq!(pass_plan.video_op_count, 1);
        assert_eq!(
            pass_plan.required_video_resources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert!(pass_plan.requires_video_decode);
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
    fn draw_pass_plan_reports_full_extent_sampled_image_fallback() {
        let mut image = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/fullscreen.png"));
        image.fit = FitMode::Cover;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "sampled-image-full-extent-fallback-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 0);
        assert!(pass_plan.sampled_image_full_extent_fallback_ready);
        assert!(!pass_plan.sampled_image_recording_ready);
        assert_eq!(pass_plan.sampled_image_recording_steps.len(), 0);
        assert_eq!(
            pass_plan.required_image_resources,
            vec![PathBuf::from("/tmp/fullscreen.png")]
        );
    }

    #[test]
    fn draw_pass_plan_reports_mixed_quad_and_full_extent_sampled_image_fallback() {
        let mut image = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/fullscreen.png"));
        image.fit = FitMode::Cover;
        let mut rectangle = draw_op(1, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rectangle.color = Some("#102030".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image, rectangle],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "mixed-quad-sampled-image-full-extent-fallback-ready"
        );
        assert_eq!(pass_plan.blocking_reason, None);
        assert_eq!(pass_plan.sampled_image_op_count, 1);
        assert_eq!(pass_plan.sampled_image_quads.len(), 0);
        assert!(pass_plan.sampled_image_full_extent_fallback_ready);
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
        let mut rectangle = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rectangle.color = Some("#102030".to_owned());
        rectangle.opacity = 0.8;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        let mut image = draw_op(1, NativeVulkanSceneLiteDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/overlay.png"));
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.opacity = 0.5;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![rectangle, image],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

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
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 80);
    }

    #[test]
    fn draw_pass_plan_reports_recordable_rectangle_and_rounded_rectangle_payload() {
        let mut rectangle = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.5;
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.transform.x = 24.0;
        let mut rounded = draw_op(1, NativeVulkanSceneLiteDrawOpKind::Rectangle);
        rounded.color = Some("#ffffff".to_owned());
        rounded.width = Some(120.0);
        rounded.height = Some(60.0);
        rounded.corner_radius = Some(8.0);
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![rectangle, rounded],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

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
            pass_plan.quad_recording_steps[0].pipeline,
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
        let mut ellipse = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Ellipse);
        ellipse.color = Some("#336699".to_owned());
        ellipse.opacity = 0.5;
        ellipse.width = Some(120.0);
        ellipse.height = Some(60.0);
        ellipse.transform.x = 10.0;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![ellipse],
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
        let mut path = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.path_data = Some("M0 0 L100 0 L100 50 L0 50 Z".to_owned());
        path.transform.x = 4.0;
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![path],
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
        let mut path = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.path_data = Some("M0 0 L100 0 L100 100 L50 50 L0 100 Z".to_owned());
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![path],
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
        assert_eq!(pass_plan.quad_recording_steps[0].kind, "path");
        assert_eq!(pass_plan.quad_recording_steps[0].vertex_count, 5);
        assert_eq!(pass_plan.quad_recording_steps[0].index_count, 9);
        assert_eq!(pass_plan.quad_vertices.len(), 5);
        assert_eq!(pass_plan.quad_indices.len(), 9);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(!pass_plan.requires_path_tessellation);
    }

    #[test]
    fn draw_pass_plan_keeps_stroked_path_pending_until_stroke_geometry_exists() {
        let mut path = draw_op(0, NativeVulkanSceneLiteDrawOpKind::Path);
        path.color = Some("#cc3300".to_owned());
        path.stroke_color = Some("#ffffff".to_owned());
        path.path_data = Some("M0 0 L100 0 L100 50 L0 50 Z".to_owned());
        let draw_plan = NativeVulkanSceneLiteDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            fallback_display_available: false,
        };

        let pass_plan = native_vulkan_scene_lite_draw_pass_plan(&draw_plan);

        assert!(pass_plan.plan_ready);
        assert!(!pass_plan.backend_ready);
        assert_eq!(
            pass_plan.backend_status,
            "draw-pass-plan-ready-recording-pending"
        );
        assert_eq!(pass_plan.recordable_op_count, 0);
        assert_eq!(pass_plan.path_op_count, 1);
        assert!(pass_plan.requires_path_tessellation);
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

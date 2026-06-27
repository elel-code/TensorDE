use std::path::PathBuf;

use crate::core::{FitMode, SceneTextAlign, SceneTransform};

use super::super::present::render_plan::{
    NativeVulkanSceneDrawOp, NativeVulkanSceneDrawOpKind, NativeVulkanSceneDrawPlan,
};

const SCENE_LITE_SOLID_QUAD_VERTEX_BYTES: u64 = 24;
const SCENE_LITE_SOLID_QUAD_INDEX_BYTES: u64 = 4;
const SCENE_LITE_ELLIPSE_SEGMENTS: usize = 48;
const SCENE_LITE_ROUNDED_RECT_CORNER_SEGMENTS: usize = 8;
const SCENE_LITE_TEXT_DEFAULT_FONT_SIZE: f64 = 24.0;
const SCENE_LITE_TEXT_GLYPH_COLUMNS: usize = 5;
const SCENE_LITE_TEXT_GLYPH_ROWS: usize = 7;
const SCENE_LITE_TEXT_GLYPH_ADVANCE_COLUMNS: f64 = 6.0;
const SCENE_LITE_TEXT_LINE_ADVANCE_ROWS: f64 = 8.0;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT: u32 = 4;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT: u32 = 6;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES: u64 = 20;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES: u64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneRecordableQuad {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) kind: &'static str,
    pub(super) color: String,
    pub(super) rgba: [f32; 4],
    pub(super) fill_color: Option<String>,
    pub(super) fill_rgba: Option<[f32; 4]>,
    pub(super) stroke_color: Option<String>,
    pub(super) stroke_rgba: Option<[f32; 4]>,
    pub(super) stroke_width: Option<f64>,
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    pub(super) corner_radius: Option<f64>,
    pub(super) text: Option<String>,
    pub(super) font_size: Option<f64>,
    pub(super) font_family: Option<String>,
    pub(super) font_weight: Option<String>,
    pub(super) text_align: Option<SceneTextAlign>,
    pub(super) path_data: Option<String>,
    pub(super) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneQuadRecordingStep {
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
    pub(super) fill_geometry: bool,
    pub(super) stroke_geometry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneSampledImageQuad {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) source: PathBuf,
    pub(super) fit: FitMode,
    pub(super) opacity: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneSampledImageRecordingStep {
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
pub(super) struct NativeVulkanSceneQuadVertex {
    pub(super) position: [f32; 2],
    pub(super) rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NativeVulkanSceneSampledImageVertex {
    pub(super) position: [f32; 2],
    pub(super) uv: [f32; 2],
    pub(super) opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneDrawPassPlan {
    pub(super) plan_ready: bool,
    pub(super) backend_ready: bool,
    pub(super) backend_status: &'static str,
    pub(super) blocking_reason: Option<&'static str>,
    pub(super) recordable_op_count: usize,
    pub(super) recordable_quads: Vec<NativeVulkanSceneRecordableQuad>,
    pub(super) quad_recording_ready: bool,
    pub(super) quad_recording_steps: Vec<NativeVulkanSceneQuadRecordingStep>,
    pub(super) quad_vertices: Vec<NativeVulkanSceneQuadVertex>,
    pub(super) quad_indices: Vec<u32>,
    pub(super) quad_vertex_buffer_bytes: u64,
    pub(super) quad_index_buffer_bytes: u64,
    pub(super) sampled_image_quads: Vec<NativeVulkanSceneSampledImageQuad>,
    pub(super) sampled_image_recording_ready: bool,
    pub(super) sampled_image_implicit_full_extent_ready: bool,
    pub(super) sampled_image_recording_steps: Vec<NativeVulkanSceneSampledImageRecordingStep>,
    pub(super) sampled_image_vertices: Vec<NativeVulkanSceneSampledImageVertex>,
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
    pub(super) requires_text_geometry: bool,
    pub(super) requires_path_tessellation: bool,
    pub(super) requires_video_decode: bool,
    pub(super) fast_clear_color: Option<String>,
}

pub(super) fn native_vulkan_scene_draw_pass_plan(
    draw_plan: &NativeVulkanSceneDrawPlan,
) -> NativeVulkanSceneDrawPassPlan {
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
            NativeVulkanSceneDrawOpKind::Image => {
                sampled_image_op_count = sampled_image_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    required_image_resources.push(source.clone());
                }
            }
            NativeVulkanSceneDrawOpKind::Video => {
                video_op_count = video_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    required_video_resources.push(source.clone());
                }
            }
            NativeVulkanSceneDrawOpKind::ColorQuad => {
                color_op_count = color_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Rectangle | NativeVulkanSceneDrawOpKind::Ellipse => {
                vector_shape_op_count = vector_shape_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Text => {
                text_op_count = text_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Path => {
                path_op_count = path_op_count.saturating_add(1);
            }
        }
    }

    let fast_clear_color = native_vulkan_scene_fast_clear_color(&draw_plan.draw_ops);
    let background_clear_color = native_vulkan_scene_background_clear_color(&draw_plan.draw_ops);
    let clear_background_op_count = usize::from(background_clear_color.is_some());
    let recordable_quads = draw_plan
        .draw_ops
        .iter()
        .filter_map(native_vulkan_scene_recordable_quad)
        .collect::<Vec<_>>();
    let recordable_op_count = recordable_quads.len();
    let quad_recording_payload = native_vulkan_scene_quad_recording_payload(&recordable_quads);
    let recorded_path_geometry_count = quad_recording_payload
        .steps
        .iter()
        .filter(|step| step.kind == "path")
        .count();
    let recorded_text_geometry_count = quad_recording_payload
        .steps
        .iter()
        .filter(|step| step.kind == "text")
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
        .filter_map(native_vulkan_scene_sampled_image_quad)
        .collect::<Vec<_>>();
    let sampled_image_recording_payload =
        native_vulkan_scene_sampled_image_recording_payload(&sampled_image_quads);
    let sampled_image_recording_ready = sampled_image_op_count > 0
        && sampled_image_recording_payload.steps.len() == sampled_image_op_count;
    let full_extent_sampled_image_op_count =
        native_vulkan_scene_full_extent_sampled_image_op_count(&draw_plan.draw_ops);
    let sampled_image_implicit_full_extent_ready =
        full_extent_sampled_image_op_count == 1 && sampled_image_op_count == 1;
    let mixed_quad_sampled_image_recording_ready = !quad_recording_payload.steps.is_empty()
        && sampled_image_recording_ready
        && quad_recording_payload
            .steps
            .len()
            .saturating_add(sampled_image_recording_payload.steps.len())
            .saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let mixed_quad_sampled_image_implicit_full_extent_ready =
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
        native_vulkan_scene_solid_vertex_buffer_bytes(quad_recording_payload.vertices.len());
    let quad_index_buffer_bytes =
        native_vulkan_scene_solid_index_buffer_bytes(quad_recording_payload.indices.len());
    let sampled_image_vertex_buffer_bytes = native_vulkan_scene_sampled_image_vertex_buffer_bytes(
        sampled_image_recording_payload.steps.len(),
    );
    let sampled_image_index_buffer_bytes = native_vulkan_scene_sampled_image_index_buffer_bytes(
        sampled_image_recording_payload.steps.len(),
    );
    let plan_ready = draw_plan.native_draw_ready();
    let single_video_scene_bridge_ready =
        video_op_count == 1 && required_video_resources.len() == 1 && draw_plan.draw_ops.len() == 1;
    let clear_background_video_scene_bridge_ready = video_op_count == 1
        && required_video_resources.len() == 1
        && clear_background_op_count == 1
        && draw_plan.draw_ops.len() == 2;
    let sampled_image_recording_complete = sampled_image_recording_ready
        && sampled_image_op_count.saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let sampled_image_implicit_full_extent_backend_ready = sampled_image_implicit_full_extent_ready
        && sampled_image_op_count.saturating_add(clear_background_op_count)
            == draw_plan.draw_ops.len();
    let backend_ready = plan_ready
        && (fast_clear_color.is_some()
            || quad_recording_ready
            || sampled_image_recording_complete
            || sampled_image_implicit_full_extent_backend_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_recording_ready
            || single_video_scene_bridge_ready
            || clear_background_video_scene_bridge_ready);
    let (backend_status, blocking_reason) = if !plan_ready {
        (
            "blocked-by-unsupported-scene-layers",
            Some("unsupported-scene-layers"),
        )
    } else if draw_plan.draw_ops.is_empty() {
        ("blocked-empty-scene-draw-plan", Some("empty-draw-plan"))
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
        } else if mixed_quad_sampled_image_implicit_full_extent_ready
            && clear_background_op_count > 0
        {
            (
                "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready",
                None,
            )
        } else if mixed_quad_sampled_image_implicit_full_extent_ready {
            ("mixed-quad-sampled-image-implicit-full-extent-ready", None)
        } else if sampled_image_implicit_full_extent_backend_ready && clear_background_op_count > 0
        {
            (
                "clear-background-sampled-image-implicit-full-extent-ready",
                None,
            )
        } else if sampled_image_implicit_full_extent_backend_ready {
            ("sampled-image-implicit-full-extent-ready", None)
        } else if sampled_image_recording_complete && clear_background_op_count > 0 {
            ("clear-background-sampled-image-recording-ready", None)
        } else if clear_background_video_scene_bridge_ready {
            (
                "clear-background-video-layer-vulkan-video-scene-bridge-ready",
                None,
            )
        } else if single_video_scene_bridge_ready {
            ("video-layer-vulkan-video-scene-bridge-ready", None)
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

    NativeVulkanSceneDrawPassPlan {
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
        sampled_image_implicit_full_extent_ready,
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
        requires_text_geometry: text_op_count > recorded_text_geometry_count,
        requires_path_tessellation: path_op_count > recorded_path_geometry_count,
        requires_video_decode: video_op_count > 0,
        fast_clear_color,
    }
}

fn native_vulkan_scene_fast_clear_color(draw_ops: &[NativeVulkanSceneDrawOp]) -> Option<String> {
    let [op] = draw_ops else {
        return None;
    };
    if op.kind != NativeVulkanSceneDrawOpKind::ColorQuad
        || op.opacity < 1.0
        || op.transform != SceneTransform::default()
    {
        return None;
    }
    op.color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned)
}

fn native_vulkan_scene_background_clear_color(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> Option<String> {
    let [op, ..] = draw_ops else {
        return None;
    };
    if draw_ops.len() <= 1
        || op.kind != NativeVulkanSceneDrawOpKind::ColorQuad
        || op.opacity < 1.0
        || op.width.is_some()
        || op.height.is_some()
        || op.transform != SceneTransform::default()
    {
        return None;
    }
    op.color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned)
}

struct NativeVulkanSceneQuadRecordingPayload {
    steps: Vec<NativeVulkanSceneQuadRecordingStep>,
    vertices: Vec<NativeVulkanSceneQuadVertex>,
    indices: Vec<u32>,
}

struct NativeVulkanSceneSampledImageRecordingPayload {
    steps: Vec<NativeVulkanSceneSampledImageRecordingStep>,
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
) -> NativeVulkanSceneSampledImageRecordingPayload {
    let mut steps = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for quad in quads
        .iter()
        .filter(|quad| native_vulkan_scene_sampled_image_quad_has_recordable_geometry(quad))
    {
        let index = steps.len();
        if let Some(quad_vertices) = native_vulkan_scene_sampled_image_vertices(quad) {
            let resource_index = index as u32;
            let first_vertex = (index as u32).saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT);
            let first_index = (index as u32).saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT);
            steps.push(NativeVulkanSceneSampledImageRecordingStep {
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
    NativeVulkanSceneSampledImageRecordingPayload {
        steps,
        vertices,
        indices,
    }
}

fn native_vulkan_scene_solid_has_recordable_geometry(
    quad: &NativeVulkanSceneRecordableQuad,
) -> bool {
    match quad.kind {
        "rectangle" | "rounded-rectangle" => {
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
        "rectangle" => native_vulkan_scene_rectangle_geometry(quad),
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
    let mut outline = Vec::with_capacity(SCENE_LITE_ELLIPSE_SEGMENTS);
    for segment in 0..SCENE_LITE_ELLIPSE_SEGMENTS {
        let theta = (segment as f64) * std::f64::consts::TAU / (SCENE_LITE_ELLIPSE_SEGMENTS as f64);
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
    let points = native_vulkan_scene_simple_path_points(quad.path_data.as_deref()?)?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    if let Some(fill_rgba) = quad.fill_rgba {
        if points.len() >= 3 {
            native_vulkan_scene_push_path_fill(
                &mut vertices,
                &mut indices,
                &points,
                fill_rgba,
                quad.transform,
            )?;
        }
    }
    if let (Some(stroke_rgba), Some(stroke_width)) = (quad.stroke_rgba, quad.stroke_width) {
        if points.len() >= 2 {
            native_vulkan_scene_push_polyline_stroke(
                &mut vertices,
                &mut indices,
                &points,
                native_vulkan_scene_simple_path_is_closed(quad.path_data.as_deref()?),
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
    let cell = font_size / SCENE_LITE_TEXT_GLYPH_ROWS as f64;
    let line_advance = cell * SCENE_LITE_TEXT_LINE_ADVANCE_ROWS;
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
                for column in 0..SCENE_LITE_TEXT_GLYPH_COLUMNS {
                    let mask = 1u8 << (SCENE_LITE_TEXT_GLYPH_COLUMNS - 1 - column);
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
            cursor_x += cell * SCENE_LITE_TEXT_GLYPH_ADVANCE_COLUMNS;
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        None
    } else {
        Some((vertices, indices))
    }
}

fn native_vulkan_scene_text_font_size(quad: &NativeVulkanSceneRecordableQuad) -> Option<f64> {
    let font_size = quad.font_size.unwrap_or(SCENE_LITE_TEXT_DEFAULT_FONT_SIZE);
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
        let columns = SCENE_LITE_TEXT_GLYPH_COLUMNS as f64
            + SCENE_LITE_TEXT_GLYPH_ADVANCE_COLUMNS * char_count.saturating_sub(1) as f64;
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

fn native_vulkan_scene_text_glyph_pattern(ch: char) -> [u8; SCENE_LITE_TEXT_GLYPH_ROWS] {
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

fn native_vulkan_scene_sampled_image_vertices(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<[NativeVulkanSceneSampledImageVertex; 4]> {
    let points = native_vulkan_scene_quad_positions(quad.width, quad.height, quad.transform)?;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    let mut vertices = [NativeVulkanSceneSampledImageVertex {
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

fn native_vulkan_scene_quad_positions(
    width: f64,
    height: f64,
    transform: SceneTransform,
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
        *position = native_vulkan_scene_transform_point_with_rotation(x, y, transform, cos, sin)?;
    }
    Some(positions)
}

fn native_vulkan_scene_transform_point(
    x: f64,
    y: f64,
    transform: SceneTransform,
) -> Option<[f32; 2]> {
    let rotation = transform.rotation_deg.to_radians();
    native_vulkan_scene_transform_point_with_rotation(
        x,
        y,
        transform,
        rotation.cos(),
        rotation.sin(),
    )
}

fn native_vulkan_scene_transform_point_with_rotation(
    x: f64,
    y: f64,
    transform: SceneTransform,
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

fn native_vulkan_scene_solid_vertex_buffer_bytes(vertex_count: usize) -> u64 {
    (vertex_count as u64).saturating_mul(SCENE_LITE_SOLID_QUAD_VERTEX_BYTES)
}

fn native_vulkan_scene_solid_index_buffer_bytes(index_count: usize) -> u64 {
    (index_count as u64).saturating_mul(SCENE_LITE_SOLID_QUAD_INDEX_BYTES)
}

fn native_vulkan_scene_sampled_image_vertex_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT))
        .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_BYTES)
}

fn native_vulkan_scene_sampled_image_index_buffer_bytes(quad_count: usize) -> u64 {
    (quad_count as u64)
        .saturating_mul(u64::from(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT))
        .saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_BYTES)
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
        _ => None,
    }
}

fn native_vulkan_scene_sampled_image_quad(
    op: &NativeVulkanSceneDrawOp,
) -> Option<NativeVulkanSceneSampledImageQuad> {
    if op.kind != NativeVulkanSceneDrawOpKind::Image {
        return None;
    }
    Some(NativeVulkanSceneSampledImageQuad {
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

fn native_vulkan_scene_full_extent_sampled_image_op_count(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> usize {
    draw_ops
        .iter()
        .filter(|op| native_vulkan_scene_full_extent_sampled_image_op_ready(op))
        .count()
}

fn native_vulkan_scene_full_extent_sampled_image_op_ready(op: &NativeVulkanSceneDrawOp) -> bool {
    op.kind == NativeVulkanSceneDrawOpKind::Image
        && op.source.is_some()
        && op.opacity == 1.0
        && op.width.is_none()
        && op.height.is_none()
        && op.transform == SceneTransform::default()
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
        font_weight: op.font_weight.clone(),
        text_align: op.text_align,
        path_data: op.path_data.clone(),
        transform: op.transform,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeVulkanScenePathToken {
    Command(char),
    Number(f64),
}

fn native_vulkan_scene_simple_path_points(path: &str) -> Option<Vec<[f64; 2]>> {
    let tokens = native_vulkan_scene_path_tokens(path)?;
    let mut index = 0usize;
    let mut command = None::<char>;
    let mut points = Vec::new();
    let mut current = [0.0, 0.0];
    let mut start = [0.0, 0.0];
    let mut started = false;

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
                        if started && points.len() >= 3 {
                            return None;
                        }
                        start = point;
                        started = true;
                        first = false;
                    }
                    points.push(point);
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
                    points.push(current);
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
                    points.push(current);
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
                    points.push(current);
                    if index < tokens.len()
                        && matches!(tokens[index], NativeVulkanScenePathToken::Command(_))
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
            'M' | 'm' | 'L' | 'l' | 'H' | 'h' | 'V' | 'v' | 'Z' | 'z'
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

fn native_vulkan_scene_simple_path_is_closed(path: &str) -> bool {
    native_vulkan_scene_path_tokens(path).is_some_and(|tokens| {
        tokens
            .iter()
            .any(|token| matches!(token, NativeVulkanScenePathToken::Command('Z' | 'z')))
    })
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

fn native_vulkan_scene_rectangle_recordable_kind(op: &NativeVulkanSceneDrawOp) -> &'static str {
    if op
        .corner_radius
        .is_some_and(|radius| radius.is_finite() && radius > 0.0)
    {
        "rounded-rectangle"
    } else {
        "rectangle"
    }
}

fn native_vulkan_scene_rgba_from_hex(color: &str, opacity: f64) -> Option<[f32; 4]> {
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

    fn draw_op(layer_index: usize, kind: NativeVulkanSceneDrawOpKind) -> NativeVulkanSceneDrawOp {
        NativeVulkanSceneDrawOp {
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
            transform: SceneTransform::default(),
        }
    }

    #[test]
    fn draw_pass_plan_reports_fast_clear_color_ready() {
        let mut color = draw_op(0, NativeVulkanSceneDrawOpKind::ColorQuad);
        color.color = Some("#102030".to_owned());
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![color],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![image, text, path],
            unsupported_layers: Vec::new(),
            manifest_preview_available: true,
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
            draw_ops: vec![video],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
    fn draw_pass_plan_reports_sampled_image_quad_payload() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));
        image.fit = FitMode::Contain;
        image.opacity = 0.75;
        image.width = Some(320.0);
        image.height = Some(180.0);
        image.transform.x = 16.0;
        image.transform.y = 8.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
    fn draw_pass_plan_reports_implicit_full_extent_sampled_image() {
        let mut image = draw_op(0, NativeVulkanSceneDrawOpKind::Image);
        image.source = Some(PathBuf::from("/tmp/fullscreen.png"));
        image.fit = FitMode::Cover;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![image],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![image, rectangle],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![rectangle, image],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
        assert_eq!(pass_plan.sampled_image_vertex_buffer_bytes, 80);
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
            draw_ops: vec![rectangle, rounded],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
        let mut ellipse = draw_op(0, NativeVulkanSceneDrawOpKind::Ellipse);
        ellipse.color = Some("#336699".to_owned());
        ellipse.opacity = 0.5;
        ellipse.width = Some(120.0);
        ellipse.height = Some(60.0);
        ellipse.transform.x = 10.0;
        let draw_plan = NativeVulkanSceneDrawPlan {
            snapshot_time_ms: 0,
            draw_ops: vec![ellipse],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![text],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![path],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![rectangle, ellipse],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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
            draw_ops: vec![rectangle],
            unsupported_layers: Vec::new(),
            manifest_preview_available: false,
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

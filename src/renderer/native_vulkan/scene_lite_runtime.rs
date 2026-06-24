use serde::Serialize;
use std::path::PathBuf;

use crate::core::{FitMode, SceneLiteTextAlign, SceneLiteTransform};

use super::NativeVulkanRenderItem;
use super::render_plan::native_vulkan_scene_lite_draw_plan;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneLiteRuntimeSnapshot {
    pub snapshot_time_ms: u64,
    pub native_draw_ready: bool,
    pub fallback_display_available: bool,
    pub draw_op_count: usize,
    pub unsupported_layer_count: usize,
    pub draw_ops: Vec<NativeVulkanSceneLiteDrawOpSnapshot>,
    pub unsupported_layers: Vec<NativeVulkanSceneLiteUnsupportedLayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneLiteDrawOpSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub kind: &'static str,
    pub opacity: f64,
    pub source: Option<PathBuf>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneLiteTextAlign>,
    pub path_data: Option<String>,
    pub fit: FitMode,
    pub transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneLiteUnsupportedLayerSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub reason: &'static str,
}

pub(super) fn native_vulkan_scene_lite_runtime_snapshot(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanSceneLiteRuntimeSnapshot> {
    let plan = native_vulkan_scene_lite_draw_plan(render_item)?;
    Some(NativeVulkanSceneLiteRuntimeSnapshot {
        snapshot_time_ms: plan.snapshot_time_ms,
        native_draw_ready: plan.native_draw_ready(),
        fallback_display_available: plan.fallback_display_available,
        draw_op_count: plan.draw_ops.len(),
        unsupported_layer_count: plan.unsupported_layers.len(),
        draw_ops: plan
            .draw_ops
            .into_iter()
            .map(|op| NativeVulkanSceneLiteDrawOpSnapshot {
                layer_index: op.layer_index,
                layer_id: op.layer_id,
                kind: op.kind.as_str(),
                opacity: op.opacity,
                source: op.source,
                color: op.color,
                stroke_color: op.stroke_color,
                stroke_width: op.stroke_width,
                corner_radius: op.corner_radius,
                width: op.width,
                height: op.height,
                text: op.text,
                font_size: op.font_size,
                font_family: op.font_family,
                font_weight: op.font_weight,
                text_align: op.text_align,
                path_data: op.path_data,
                fit: op.fit,
                transform: op.transform,
            })
            .collect(),
        unsupported_layers: plan
            .unsupported_layers
            .into_iter()
            .map(|layer| NativeVulkanSceneLiteUnsupportedLayerSnapshot {
                layer_index: layer.layer_index,
                layer_id: layer.layer_id,
                reason: layer.reason,
            })
            .collect(),
    })
}

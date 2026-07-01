use std::path::PathBuf;

use crate::core::{SceneNodeKind, SceneTransform};
use crate::renderer::SceneRenderLayer;

use super::super::super::present::render_plan::{
    NativeVulkanSceneDrawOp, NativeVulkanSceneDrawOpKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanSceneDrawPassOpBuckets {
    pub(super) color_op_count: usize,
    pub(super) sampled_image_op_count: usize,
    pub(super) video_op_count: usize,
    pub(super) vector_shape_op_count: usize,
    pub(super) text_op_count: usize,
    pub(super) path_op_count: usize,
    pub(super) required_image_resources: Vec<PathBuf>,
    pub(super) required_video_resources: Vec<PathBuf>,
}

pub(super) fn native_vulkan_scene_draw_pass_op_buckets(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> NativeVulkanSceneDrawPassOpBuckets {
    let mut buckets = NativeVulkanSceneDrawPassOpBuckets {
        color_op_count: 0,
        sampled_image_op_count: 0,
        video_op_count: 0,
        vector_shape_op_count: 0,
        text_op_count: 0,
        path_op_count: 0,
        required_image_resources: Vec::new(),
        required_video_resources: Vec::new(),
    };

    for op in draw_ops {
        match op.kind {
            NativeVulkanSceneDrawOpKind::Image => {
                buckets.sampled_image_op_count = buckets.sampled_image_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    native_vulkan_scene_push_unique_path(
                        &mut buckets.required_image_resources,
                        source,
                    );
                }
            }
            NativeVulkanSceneDrawOpKind::Video => {
                buckets.video_op_count = buckets.video_op_count.saturating_add(1);
                if let Some(source) = &op.source {
                    native_vulkan_scene_push_unique_path(
                        &mut buckets.required_video_resources,
                        source,
                    );
                }
            }
            NativeVulkanSceneDrawOpKind::ColorQuad => {
                buckets.color_op_count = buckets.color_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Rectangle
            | NativeVulkanSceneDrawOpKind::Ellipse
            | NativeVulkanSceneDrawOpKind::AudioResponse => {
                buckets.vector_shape_op_count = buckets.vector_shape_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Text => {
                buckets.text_op_count = buckets.text_op_count.saturating_add(1);
            }
            NativeVulkanSceneDrawOpKind::Path => {
                buckets.path_op_count = buckets.path_op_count.saturating_add(1);
            }
        }
    }

    buckets
}

pub(super) fn native_vulkan_scene_fast_clear_color(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> Option<String> {
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

pub(super) fn native_vulkan_scene_background_clear_color(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> Option<String> {
    let [op, ..] = draw_ops else {
        return None;
    };
    if draw_ops.len() <= 1
        || op.kind != NativeVulkanSceneDrawOpKind::ColorQuad
        || op.opacity < 1.0
        || (!native_vulkan_scene_render_clear_op(op) && (op.width.is_some() || op.height.is_some()))
        || op.transform != SceneTransform::default()
    {
        return None;
    }
    op.color
        .as_deref()
        .filter(|color| !color.is_empty())
        .map(str::to_owned)
}

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_render_layer_is_clear(
    layer: &SceneRenderLayer,
) -> bool {
    layer.id == "scene-render-clear-color"
        && layer.kind == SceneNodeKind::Color
        && layer.opacity >= 1.0
        && layer.transform == SceneTransform::default()
}

pub(super) fn native_vulkan_scene_full_extent_sampled_image_op_count(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> usize {
    draw_ops
        .iter()
        .filter(|op| native_vulkan_scene_full_extent_sampled_image_op_ready(op))
        .count()
}

fn native_vulkan_scene_render_clear_op(op: &NativeVulkanSceneDrawOp) -> bool {
    op.layer_id == "scene-render-clear-color"
}

pub(super) fn native_vulkan_scene_full_extent_sampled_image_op_ready(
    op: &NativeVulkanSceneDrawOp,
) -> bool {
    op.kind == NativeVulkanSceneDrawOpKind::Image
        && op.source.is_some()
        && op.mesh.is_none()
        && op.opacity == 1.0
        && op.width.is_none()
        && op.height.is_none()
        && op.transform == SceneTransform::default()
}

fn native_vulkan_scene_push_unique_path(paths: &mut Vec<PathBuf>, source: &PathBuf) {
    if !paths.iter().any(|existing| existing == source) {
        paths.push(source.clone());
    }
}

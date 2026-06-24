use std::path::PathBuf;

use crate::core::SceneLiteTransform;

use super::render_plan::{
    NativeVulkanSceneLiteDrawOp, NativeVulkanSceneLiteDrawOpKind, NativeVulkanSceneLiteDrawPlan,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteDrawPassPlan {
    pub(super) plan_ready: bool,
    pub(super) backend_ready: bool,
    pub(super) backend_status: &'static str,
    pub(super) blocking_reason: Option<&'static str>,
    pub(super) recordable_op_count: usize,
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
    let recordable_op_count = usize::from(fast_clear_color.is_some());
    let plan_ready = draw_plan.native_draw_ready();
    let backend_ready = plan_ready && fast_clear_color.is_some();
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
        ("fast-clear-color-ready", None)
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
}

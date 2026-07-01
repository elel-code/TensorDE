use crate::core::SceneBlendMode;

use super::{
    NativeVulkanSceneBlendEquation, NativeVulkanSceneBlendFactor, NativeVulkanSceneBlendOp,
    NativeVulkanSceneBlendState, NativeVulkanSceneCullMode, NativeVulkanSceneMaterialFlag,
    NativeVulkanSceneRenderState,
};

pub(super) fn native_vulkan_scene_blend_state(mode: SceneBlendMode) -> NativeVulkanSceneBlendState {
    NativeVulkanSceneBlendState {
        mode,
        equation: native_vulkan_scene_blend_equation(mode),
    }
}

pub(super) fn native_vulkan_scene_render_state(
    blend_mode: SceneBlendMode,
    depth_test: NativeVulkanSceneMaterialFlag,
    depth_write: NativeVulkanSceneMaterialFlag,
    cull_mode: NativeVulkanSceneCullMode,
) -> NativeVulkanSceneRenderState {
    NativeVulkanSceneRenderState {
        blend: native_vulkan_scene_blend_state(blend_mode),
        depth_test,
        depth_write,
        cull_mode,
    }
}

pub(super) fn native_vulkan_scene_solid_quad_pipeline_label(
    blend: NativeVulkanSceneBlendState,
) -> &'static str {
    match blend.mode {
        SceneBlendMode::Alpha => "solid-quad-alpha-blend",
        SceneBlendMode::Additive => "solid-quad-additive-blend",
        SceneBlendMode::Multiply => "solid-quad-multiply-blend",
        SceneBlendMode::Screen => "solid-quad-screen-blend",
        SceneBlendMode::Max => "solid-quad-max-blend",
    }
}

pub(super) fn native_vulkan_scene_blend_equation_label(
    blend: NativeVulkanSceneBlendState,
) -> String {
    format!(
        "color={}*src {} {}*dst alpha={}*src {} {}*dst",
        blend.equation.src_color.as_str(),
        blend.equation.color_op.as_str(),
        blend.equation.dst_color.as_str(),
        blend.equation.src_alpha.as_str(),
        blend.equation.alpha_op.as_str(),
        blend.equation.dst_alpha.as_str(),
    )
}

pub(super) fn native_vulkan_scene_sampled_image_pipeline_label(
    render_state: &NativeVulkanSceneRenderState,
) -> &'static str {
    match render_state.blend.mode {
        SceneBlendMode::Alpha => "sampled-image-alpha-blend",
        SceneBlendMode::Additive => "sampled-image-additive-blend",
        SceneBlendMode::Multiply => "sampled-image-multiply-blend",
        SceneBlendMode::Screen => "sampled-image-screen-blend",
        SceneBlendMode::Max => "sampled-image-max-blend",
    }
}

fn native_vulkan_scene_blend_equation(mode: SceneBlendMode) -> NativeVulkanSceneBlendEquation {
    match mode {
        SceneBlendMode::Alpha => NativeVulkanSceneBlendEquation {
            src_color: NativeVulkanSceneBlendFactor::SrcAlpha,
            dst_color: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            color_op: NativeVulkanSceneBlendOp::Add,
            src_alpha: NativeVulkanSceneBlendFactor::SrcAlpha,
            dst_alpha: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            alpha_op: NativeVulkanSceneBlendOp::Add,
        },
        SceneBlendMode::Additive => NativeVulkanSceneBlendEquation {
            src_color: NativeVulkanSceneBlendFactor::SrcAlpha,
            dst_color: NativeVulkanSceneBlendFactor::One,
            color_op: NativeVulkanSceneBlendOp::Add,
            src_alpha: NativeVulkanSceneBlendFactor::One,
            dst_alpha: NativeVulkanSceneBlendFactor::One,
            alpha_op: NativeVulkanSceneBlendOp::Add,
        },
        SceneBlendMode::Multiply => NativeVulkanSceneBlendEquation {
            src_color: NativeVulkanSceneBlendFactor::DstColor,
            dst_color: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            color_op: NativeVulkanSceneBlendOp::Add,
            src_alpha: NativeVulkanSceneBlendFactor::One,
            dst_alpha: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            alpha_op: NativeVulkanSceneBlendOp::Add,
        },
        SceneBlendMode::Screen => NativeVulkanSceneBlendEquation {
            src_color: NativeVulkanSceneBlendFactor::OneMinusDstColor,
            dst_color: NativeVulkanSceneBlendFactor::One,
            color_op: NativeVulkanSceneBlendOp::Add,
            src_alpha: NativeVulkanSceneBlendFactor::One,
            dst_alpha: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            alpha_op: NativeVulkanSceneBlendOp::Add,
        },
        SceneBlendMode::Max => NativeVulkanSceneBlendEquation {
            src_color: NativeVulkanSceneBlendFactor::One,
            dst_color: NativeVulkanSceneBlendFactor::One,
            color_op: NativeVulkanSceneBlendOp::Max,
            src_alpha: NativeVulkanSceneBlendFactor::One,
            dst_alpha: NativeVulkanSceneBlendFactor::OneMinusSrcAlpha,
            alpha_op: NativeVulkanSceneBlendOp::Add,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_state_owns_vulkan_equations() {
        let alpha = native_vulkan_scene_blend_state(SceneBlendMode::Alpha);
        assert_eq!(
            alpha.equation.src_color,
            NativeVulkanSceneBlendFactor::SrcAlpha
        );
        assert_eq!(
            alpha.equation.dst_color,
            NativeVulkanSceneBlendFactor::OneMinusSrcAlpha
        );
        assert_eq!(alpha.equation.color_op, NativeVulkanSceneBlendOp::Add);

        let multiply = native_vulkan_scene_blend_state(SceneBlendMode::Multiply);
        assert_eq!(
            multiply.equation.src_color,
            NativeVulkanSceneBlendFactor::DstColor
        );
        assert_eq!(
            multiply.equation.dst_color,
            NativeVulkanSceneBlendFactor::OneMinusSrcAlpha
        );

        let screen = native_vulkan_scene_blend_state(SceneBlendMode::Screen);
        assert_eq!(
            screen.equation.src_color,
            NativeVulkanSceneBlendFactor::OneMinusDstColor
        );

        let max = native_vulkan_scene_blend_state(SceneBlendMode::Max);
        assert_eq!(max.equation.color_op, NativeVulkanSceneBlendOp::Max);
        assert_eq!(
            native_vulkan_scene_blend_equation_label(max),
            "color=one*src max one*dst alpha=one*src add one-minus-src-alpha*dst"
        );
    }
}

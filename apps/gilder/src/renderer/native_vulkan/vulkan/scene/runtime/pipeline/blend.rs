//! Vulkan fixed-function blend state lowering for scene pipelines.

use super::*;

pub(super) fn scene_color_blend_attachment(
    blend: SceneGpuBlend,
    color_write_mask: SceneColorWriteMask,
) -> vk::PipelineColorBlendAttachmentState {
    let mut components = vk::ColorComponentFlags::R
        | vk::ColorComponentFlags::G
        | vk::ColorComponentFlags::B;
    if color_write_mask == SceneColorWriteMask::Rgba {
        components |= vk::ColorComponentFlags::A;
    }
    let builder =
        vk::PipelineColorBlendAttachmentState::builder().color_write_mask(components);
    match blend {
        SceneGpuBlend::Replace | SceneGpuBlend::AlphaToCoverage => {
            builder.blend_enable(false).build()
        }
        SceneGpuBlend::Additive => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::Multiply => advanced_blend_attachment(builder, vk::BlendOp::MULTIPLY_EXT),
        SceneGpuBlend::MultiplyPremultiplied => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::Screen => advanced_blend_attachment(builder, vk::BlendOp::SCREEN_EXT),
        SceneGpuBlend::ScreenPremultiplied => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_COLOR)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::HslColor => advanced_blend_attachment(builder, vk::BlendOp::HSL_COLOR_EXT),
        SceneGpuBlend::Maximum => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::MAX)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::MAX)
            .build(),
        SceneGpuBlend::Modulate => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::Alpha => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
    }
}

fn advanced_blend_attachment(
    builder: vk::PipelineColorBlendAttachmentStateBuilder,
    operation: vk::BlendOp,
) -> vk::PipelineColorBlendAttachmentState {
    builder
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ZERO)
        .color_blend_op(operation)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(operation)
        .build()
}

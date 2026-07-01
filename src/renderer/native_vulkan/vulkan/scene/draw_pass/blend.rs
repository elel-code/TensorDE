use vulkanalia::vk::{self, HasBuilder};

use crate::core::SceneBlendMode;

use super::{
    VulkanaliaSceneSampledImagePipelineResources, VulkanaliaSceneSolidQuadPipelineResources,
};

pub(super) fn native_vulkan_vulkanalia_scene_color_attachment(
    blend_mode: SceneBlendMode,
) -> vk::PipelineColorBlendAttachmentState {
    let builder = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )
        .blend_enable(true);
    match blend_mode {
        SceneBlendMode::Alpha => builder
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendMode::Additive => builder
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendMode::Multiply => builder
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendMode::Screen => builder
            .src_color_blend_factor(vk::BlendFactor::ONE_MINUS_DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendMode::Max => builder
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::MAX)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
    }
}

pub(super) fn native_vulkan_vulkanalia_scene_fragment_module_for_blend(
    blend_mode: SceneBlendMode,
    straight_fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
) -> vk::ShaderModule {
    match blend_mode {
        SceneBlendMode::Alpha | SceneBlendMode::Additive => straight_fragment_module,
        SceneBlendMode::Multiply | SceneBlendMode::Screen | SceneBlendMode::Max => {
            premultiplied_fragment_module
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_scene_solid_quad_pipeline(
    resources: &VulkanaliaSceneSolidQuadPipelineResources,
    blend_mode: SceneBlendMode,
) -> vk::Pipeline {
    match blend_mode {
        SceneBlendMode::Alpha => resources.alpha_pipeline,
        SceneBlendMode::Additive => resources.additive_pipeline,
        SceneBlendMode::Multiply => resources.multiply_pipeline,
        SceneBlendMode::Screen => resources.screen_pipeline,
        SceneBlendMode::Max => resources.max_pipeline,
    }
}

pub(super) fn native_vulkan_vulkanalia_scene_sampled_image_pipeline(
    resources: &VulkanaliaSceneSampledImagePipelineResources,
    blend_mode: SceneBlendMode,
) -> vk::Pipeline {
    match blend_mode {
        SceneBlendMode::Alpha => resources.alpha_pipeline,
        SceneBlendMode::Additive => resources.additive_pipeline,
        SceneBlendMode::Multiply => resources.multiply_pipeline,
        SceneBlendMode::Screen => resources.screen_pipeline,
        SceneBlendMode::Max => resources.max_pipeline,
    }
}

pub(super) fn native_vulkan_vulkanalia_scene_blend_mode_label(
    blend_mode: SceneBlendMode,
) -> &'static str {
    match blend_mode {
        SceneBlendMode::Alpha => "alpha",
        SceneBlendMode::Additive => "additive",
        SceneBlendMode::Multiply => "multiply",
        SceneBlendMode::Screen => "screen",
        SceneBlendMode::Max => "max",
    }
}

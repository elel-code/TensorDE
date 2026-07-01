use crate::core::SceneBlendMode;

pub(super) fn native_vulkan_scene_solid_quad_pipeline_label(
    blend_mode: SceneBlendMode,
) -> &'static str {
    match blend_mode {
        SceneBlendMode::Alpha => "solid-quad-alpha-blend",
        SceneBlendMode::Additive => "solid-quad-additive-blend",
        SceneBlendMode::Multiply => "solid-quad-multiply-blend",
        SceneBlendMode::Screen => "solid-quad-screen-blend",
        SceneBlendMode::Max => "solid-quad-max-blend",
    }
}

pub(super) fn native_vulkan_scene_sampled_image_pipeline_label(
    blend_mode: SceneBlendMode,
) -> &'static str {
    match blend_mode {
        SceneBlendMode::Alpha => "sampled-image-alpha-blend",
        SceneBlendMode::Additive => "sampled-image-additive-blend",
        SceneBlendMode::Multiply => "sampled-image-multiply-blend",
        SceneBlendMode::Screen => "sampled-image-screen-blend",
        SceneBlendMode::Max => "sampled-image-max-blend",
    }
}

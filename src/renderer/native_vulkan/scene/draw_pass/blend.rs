use crate::core::SceneBlendMode;

use super::{
    NativeVulkanSceneBlendState, NativeVulkanSceneCullMode, NativeVulkanSceneMaterialFlag,
    NativeVulkanSceneRenderState,
};

pub(super) fn native_vulkan_scene_blend_state(mode: SceneBlendMode) -> NativeVulkanSceneBlendState {
    NativeVulkanSceneBlendState { mode }
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

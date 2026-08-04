//! Wallpaper Engine material and object blend-state lowering.

use crate::core::SceneBlendMode;
use crate::engine::scene::abi::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

pub(super) fn pipeline_blend_from_we(value: Option<&str>) -> ScenePipelineBlend {
    match value.unwrap_or("normal").to_ascii_lowercase().as_str() {
        "translucent" | "alpha" => ScenePipelineBlend::Translucent,
        "additive" | "add" => ScenePipelineBlend::Additive,
        "disabled" | "opaque" => ScenePipelineBlend::Disabled,
        "alphatocoverage" | "alpha-to-coverage" => ScenePipelineBlend::AlphaToCoverage,
        _ => ScenePipelineBlend::Normal,
    }
}

pub(super) fn pipeline_blend_string(value: ScenePipelineBlend) -> String {
    match value {
        ScenePipelineBlend::Normal => "normal",
        ScenePipelineBlend::Translucent => "translucent",
        ScenePipelineBlend::Additive => "additive",
        ScenePipelineBlend::Disabled => "disabled",
        ScenePipelineBlend::AlphaToCoverage => "alphatocoverage",
    }
    .to_owned()
}

pub(super) fn depth_test_from_we(value: Option<&str>) -> SceneDepthTest {
    match value.unwrap_or("disabled").to_ascii_lowercase().as_str() {
        "enabled" => SceneDepthTest::Enabled,
        _ => SceneDepthTest::Disabled,
    }
}

pub(super) fn cull_mode_from_we(value: Option<&str>) -> SceneCullMode {
    match value.unwrap_or("nocull").to_ascii_lowercase().as_str() {
        "normal" => SceneCullMode::Normal,
        _ => SceneCullMode::None,
    }
}

pub(super) fn scene_blend_from_color_blend_mode(value: i32) -> SceneBlendMode {
    match value {
        2 | 3 => SceneBlendMode::Multiply,
        6 => SceneBlendMode::Max,
        7 | 8 => SceneBlendMode::Screen,
        28 => SceneBlendMode::HslColor,
        31 => SceneBlendMode::Additive,
        32 => SceneBlendMode::Modulate,
        _ => SceneBlendMode::Alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_we_object_blends_remain_typed() {
        assert_eq!(
            scene_blend_from_color_blend_mode(2),
            SceneBlendMode::Multiply
        );
        assert_eq!(scene_blend_from_color_blend_mode(7), SceneBlendMode::Screen);
        assert_eq!(
            scene_blend_from_color_blend_mode(31),
            SceneBlendMode::Additive
        );
    }
}

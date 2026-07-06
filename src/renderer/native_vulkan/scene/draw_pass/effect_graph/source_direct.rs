use super::native_vulkan_scene_we_image_pass_is_vertex_skew_effect_record;
use crate::renderer::native_vulkan::scene::draw_pass::{
    NativeVulkanSceneEffectKind, NativeVulkanSceneEffectRecord, NativeVulkanSceneSampledImageQuad,
};

pub(super) fn native_vulkan_scene_we_image_pass_chain_source_direct_start_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    first_class_target: bool,
    color_blend_passthrough: bool,
    material_graph_supported: bool,
) -> Option<Vec<&'static str>> {
    if quad.effect_passes.len() < 2 {
        return None;
    }
    let mut reasons = Vec::new();
    if first_class_target {
        reasons.push("first-class-target");
    }
    if color_blend_passthrough {
        reasons.push("color-blend-passthrough");
    }
    if !material_graph_supported {
        reasons.push("material-graph-unsupported");
    }
    if quad.texture_region.is_some() {
        reasons.push("texture-region");
    }
    if quad.material_pass.alpha_texture_slot.is_some() {
        reasons.push("material-alpha-texture");
    }
    native_vulkan_scene_we_image_pass_source_direct_ineligible_reasons(
        &quad.effect_passes[0],
        &mut reasons,
    );
    native_vulkan_scene_we_image_pass_chain_quad_source_direct_ineligible_reasons(
        quad,
        &quad.effect_passes[0],
        &mut reasons,
    );
    Some(reasons)
}

pub(super) fn native_vulkan_scene_we_image_pass_chain_can_sample_source_directly(
    quad: &NativeVulkanSceneSampledImageQuad,
    effect: &NativeVulkanSceneEffectRecord,
) -> bool {
    let mut reasons = Vec::new();
    native_vulkan_scene_we_image_pass_source_direct_ineligible_reasons(effect, &mut reasons);
    native_vulkan_scene_we_image_pass_chain_quad_source_direct_ineligible_reasons(
        quad,
        effect,
        &mut reasons,
    );
    reasons.is_empty()
}

fn native_vulkan_scene_we_image_pass_source_direct_ineligible_reasons(
    effect: &NativeVulkanSceneEffectRecord,
    reasons: &mut Vec<&'static str>,
) {
    if effect.target.is_some() {
        reasons.push("first-effect-explicit-target");
    }
    if effect.source.is_some() {
        reasons.push("first-effect-explicit-source");
    }
    if !effect.fbos.is_empty() {
        reasons.push("first-effect-fbos");
    }
    if effect.binds.contains_key(&0) {
        reasons.push("first-effect-bind0");
    }
    if effect.kind == NativeVulkanSceneEffectKind::Skew
        && native_vulkan_scene_we_image_pass_is_vertex_skew_effect_record(effect)
    {
        reasons.push("first-effect-vertex-skew");
    }
    if !matches!(
        effect.kind,
        NativeVulkanSceneEffectKind::OpacityMask
            | NativeVulkanSceneEffectKind::WaterRipple
            | NativeVulkanSceneEffectKind::WaterWaves
            | NativeVulkanSceneEffectKind::WaterFlow
            | NativeVulkanSceneEffectKind::WaterCaustics
            | NativeVulkanSceneEffectKind::FoliageSway
            | NativeVulkanSceneEffectKind::Scroll
            | NativeVulkanSceneEffectKind::Skew
            | NativeVulkanSceneEffectKind::TechCircle
    ) {
        reasons.push("first-effect-not-source-direct-kind");
    }
}

fn native_vulkan_scene_we_image_pass_chain_quad_source_direct_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    effect: &NativeVulkanSceneEffectRecord,
    reasons: &mut Vec<&'static str>,
) {
    if effect.kind == NativeVulkanSceneEffectKind::OpacityMask
        && native_vulkan_scene_we_image_pass_chain_is_composelayer(quad)
    {
        reasons.push("composelayer-final-quad");
    }
}

fn native_vulkan_scene_we_image_pass_chain_is_composelayer(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    let normalized = quad.layer_id.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("models-util-composelayer")
        || normalized.contains("models/util/composelayer")
}

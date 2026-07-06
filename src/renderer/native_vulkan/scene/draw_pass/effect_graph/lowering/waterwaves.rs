use crate::core::SceneBlendMode;

use super::super::super::{
    NativeVulkanSceneEffectKind, NativeVulkanSceneFusedEffectKind,
    NativeVulkanSceneFusedEffectPass, NativeVulkanSceneSampledImageQuad,
    NativeVulkanSceneTextureSlot, NativeVulkanSceneWeImagePass, NativeVulkanSceneWeImagePassRole,
};
use super::super::native_vulkan_scene_we_image_graph_endpoint_name;
use super::residency::WATERWAVES3_SHADER_VARIANT_BLOCK_REASON;

pub(super) fn native_vulkan_scene_we_image_passes_waterwaves3_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
    third: &NativeVulkanSceneWeImagePass,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    native_vulkan_scene_we_image_passes_waterwaves3_surface_ineligible_reasons(
        quad,
        first,
        second,
        third,
        &mut reasons,
    );
    if first.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
        || second.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
        || third.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
    {
        reasons.push("non-effect-material");
    }
    if first.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        || second.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        || third.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
    {
        reasons.push("non-waterwaves-kind");
    }
    reasons.push(WATERWAVES3_SHADER_VARIANT_BLOCK_REASON);
    if first.fused_effect_kind.is_some()
        || second.fused_effect_kind.is_some()
        || third.fused_effect_kind.is_some()
    {
        reasons.push("already-fused");
    }
    if first.final_scene_pass || second.final_scene_pass {
        reasons.push("early-final-scene-pass");
    }
    if !first.target.is_graph_target() || !second.target.is_graph_target() {
        reasons.push("intermediate-target-not-graph");
    }
    if second.input != first.target
        || second.input_name
            != native_vulkan_scene_we_image_graph_endpoint_name(first).map(str::to_owned)
    {
        reasons.push("second-input-not-first-target");
    }
    if third.input != second.target
        || third.input_name
            != native_vulkan_scene_we_image_graph_endpoint_name(second).map(str::to_owned)
    {
        reasons.push("third-input-not-second-target");
    }
    if first.input.is_graph_target()
        && first.input == third.target
        && first.input_name == third.target_name
    {
        reasons.push("same-input-output-target");
    }
    if first.target_name.is_some() || second.target_name.is_some() || third.target_name.is_some() {
        reasons.push("explicit-target");
    }
    if first.source.is_some() || second.source.is_some() || third.source.is_some() {
        reasons.push("explicit-source");
    }
    if !first.binds.is_empty() || !second.binds.is_empty() || !third.binds.is_empty() {
        reasons.push("binds");
    }
    if !first.fbos.is_empty() || !second.fbos.is_empty() || !third.fbos.is_empty() {
        reasons.push("fbos");
    }
    if !native_vulkan_scene_we_image_passes_have_fusible_waterwaves3_texture_slots(
        first, second, third,
    ) {
        reasons.push("texture-slots");
    }
    if first.effect_uv_transform != second.effect_uv_transform
        || first.effect_uv_transform != third.effect_uv_transform
    {
        reasons.push("effect-uv-transform");
    }
    if first.scene_blend_mode != SceneBlendMode::Normal
        || second.scene_blend_mode != SceneBlendMode::Normal
    {
        reasons.push("intermediate-blend-not-normal");
    }
    if first.depth_test != second.depth_test
        || first.depth_test != third.depth_test
        || first.depth_write != second.depth_write
        || first.depth_write != third.depth_write
        || first.cull_mode != second.cull_mode
        || first.cull_mode != third.cull_mode
    {
        reasons.push("render-state-mismatch");
    }
    reasons
}

fn native_vulkan_scene_we_image_passes_waterwaves3_surface_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    _first: &NativeVulkanSceneWeImagePass,
    _second: &NativeVulkanSceneWeImagePass,
    _third: &NativeVulkanSceneWeImagePass,
    reasons: &mut Vec<&'static str>,
) {
    if quad.effect_motion.is_active() {
        reasons.push("dynamic-effect-motion");
    }
    if quad.texture_region.is_some() {
        reasons.push("texture-region");
    }
    if quad.effect_uv_space.is_some() {
        reasons.push("effect-uv-space");
    }
    if quad.effect_target_pass.is_some() {
        reasons.push("first-class-effect-target");
    }
    if quad.material_pass.alpha_texture_slot.is_some() {
        reasons.push("material-alpha-texture");
    }
    if quad.composite_key.is_some() {
        reasons.push("composite-key");
    }
    if !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= 0.0
        || quad.height <= 0.0
    {
        reasons.push("invalid-extent");
    }
}

pub(super) fn native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    native_vulkan_scene_we_image_passes_waterwaves2_surface_ineligible_reasons(
        quad,
        first,
        second,
        &mut reasons,
    );
    if first.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
        || second.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
    {
        reasons.push("non-effect-material");
    }
    if first.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        || second.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
    {
        reasons.push("non-waterwaves-kind");
    }
    if first.fused_effect_kind.is_some() || second.fused_effect_kind.is_some() {
        reasons.push("already-fused");
    }
    if first.final_scene_pass {
        reasons.push("first-final-scene-pass");
    }
    if !first.target.is_graph_target() {
        reasons.push("first-target-not-graph");
    }
    if second.input != first.target
        || second.input_name
            != native_vulkan_scene_we_image_graph_endpoint_name(first).map(str::to_owned)
    {
        reasons.push("second-input-not-first-target");
    }
    if first.input.is_graph_target()
        && first.input == second.target
        && first.input_name == second.target_name
    {
        reasons.push("same-input-output-target");
    }
    if first.target_name.is_some() || second.target_name.is_some() {
        reasons.push("explicit-target");
    }
    if first.source.is_some() || second.source.is_some() {
        reasons.push("explicit-source");
    }
    if !first.binds.is_empty() || !second.binds.is_empty() {
        reasons.push("binds");
    }
    if !first.fbos.is_empty() || !second.fbos.is_empty() {
        reasons.push("fbos");
    }
    if !native_vulkan_scene_we_image_passes_have_fusible_waterwaves_texture_slots(first, second) {
        reasons.push("texture-slots");
    }
    if first.effect_uv_transform != second.effect_uv_transform {
        reasons.push("effect-uv-transform");
    }
    if first.scene_blend_mode != SceneBlendMode::Normal {
        reasons.push("first-blend-not-normal");
    }
    if first.depth_test != second.depth_test
        || first.depth_write != second.depth_write
        || first.cull_mode != second.cull_mode
    {
        reasons.push("render-state-mismatch");
    }
    reasons
}

fn native_vulkan_scene_we_image_passes_waterwaves2_surface_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    _first: &NativeVulkanSceneWeImagePass,
    _second: &NativeVulkanSceneWeImagePass,
    reasons: &mut Vec<&'static str>,
) {
    if quad.effect_motion.is_active() {
        reasons.push("dynamic-effect-motion");
    }
    if quad.texture_region.is_some() {
        reasons.push("texture-region");
    }
    if quad.effect_uv_space.is_some() {
        reasons.push("effect-uv-space");
    }
    if quad.effect_target_pass.is_some() {
        reasons.push("first-class-effect-target");
    }
    if quad.material_pass.alpha_texture_slot.is_some() {
        reasons.push("material-alpha-texture");
    }
    if quad.composite_key.is_some() {
        reasons.push("composite-key");
    }
    if !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= 0.0
        || quad.height <= 0.0
    {
        reasons.push("invalid-extent");
    }
}

fn native_vulkan_scene_we_image_passes_have_fusible_waterwaves_texture_slots(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> bool {
    first
        .texture_slots
        .iter()
        .chain(second.texture_slots.iter())
        .all(|slot| matches!(slot.slot, 1 | 2))
}

fn native_vulkan_scene_we_image_passes_have_fusible_waterwaves3_texture_slots(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
    third: &NativeVulkanSceneWeImagePass,
) -> bool {
    first
        .texture_slots
        .iter()
        .chain(second.texture_slots.iter())
        .chain(third.texture_slots.iter())
        .all(|slot| matches!(slot.slot, 1 | 2))
}

pub(super) fn native_vulkan_scene_we_image_pass_fused_waterwaves2(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> NativeVulkanSceneWeImagePass {
    let first_texture_slots = native_vulkan_scene_fused_waterwaves_texture_slots(first, 0);
    let second_texture_slots = native_vulkan_scene_fused_waterwaves_texture_slots(second, 2);
    let texture_slots = first_texture_slots
        .iter()
        .chain(second_texture_slots.iter())
        .cloned()
        .collect::<Vec<_>>();
    NativeVulkanSceneWeImagePass {
        pass_index: first.pass_index,
        role: NativeVulkanSceneWeImagePassRole::EffectMaterial,
        effect_kind: Some(NativeVulkanSceneEffectKind::WaterWaves),
        fused_effect_kind: Some(NativeVulkanSceneFusedEffectKind::WaterWaves2),
        fused_effect_passes: vec![
            native_vulkan_scene_fused_effect_pass_from_we_pass(first, first_texture_slots),
            native_vulkan_scene_fused_effect_pass_from_we_pass(second, second_texture_slots),
        ],
        effect_file: first.effect_file.clone(),
        command: None,
        source: None,
        target_name: second.target_name.clone(),
        binds: Default::default(),
        fbos: Default::default(),
        shader: first.shader.clone(),
        blending: second.blending.clone(),
        scene_blend_mode: second.scene_blend_mode,
        render_state: second.render_state.clone(),
        input: first.input,
        input_name: first.input_name.clone(),
        target: second.target,
        final_scene_pass: second.final_scene_pass,
        texture_slot_count: texture_slots.len(),
        texture_slots,
        effect_uv_transform: first.effect_uv_transform,
        parameter_keys: first.parameter_keys.clone(),
        constant_shader_values: first.constant_shader_values.clone(),
        combo_keys: first.combo_keys.clone(),
        combo_values: first.combo_values.clone(),
        foliage_sway_vertex_strength_model: Default::default(),
        depth_test: second.depth_test,
        depth_write: second.depth_write,
        cull_mode: second.cull_mode.clone(),
    }
}

fn native_vulkan_scene_fused_waterwaves_texture_slots(
    pass: &NativeVulkanSceneWeImagePass,
    slot_offset: u32,
) -> Vec<NativeVulkanSceneTextureSlot> {
    pass.texture_slots
        .iter()
        .filter_map(|slot| {
            if !matches!(slot.slot, 1 | 2) {
                return None;
            }
            let mut remapped = slot.clone();
            remapped.slot = remapped.slot.saturating_add(slot_offset);
            Some(remapped)
        })
        .collect()
}

fn native_vulkan_scene_fused_effect_pass_from_we_pass(
    pass: &NativeVulkanSceneWeImagePass,
    texture_slots: Vec<NativeVulkanSceneTextureSlot>,
) -> NativeVulkanSceneFusedEffectPass {
    NativeVulkanSceneFusedEffectPass {
        pass_index: pass.pass_index,
        effect_kind: pass
            .effect_kind
            .expect("fused WE effect pass requires an effect kind"),
        effect_file: pass.effect_file.clone(),
        texture_slots,
        effect_uv_transform: pass.effect_uv_transform,
        constant_shader_values: pass.constant_shader_values.clone(),
        combo_keys: pass.combo_keys.clone(),
        combo_values: pass.combo_values.clone(),
    }
}

use std::path::PathBuf;

use crate::core::SceneBlendMode;
use crate::core::scene::{SceneImageEffectPass, SceneTextureSlot};
use crate::renderer::{
    SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderTextureSlot,
};

use super::blend::{
    native_vulkan_scene_blend_equation_label, native_vulkan_scene_render_state,
    native_vulkan_scene_sampled_image_pipeline_label,
};
use super::{
    NativeVulkanSceneCullMode, NativeVulkanSceneEffectEvaluationBoundary,
    NativeVulkanSceneEffectKind, NativeVulkanSceneEffectRecord, NativeVulkanSceneMaterialFlag,
    NativeVulkanSceneMaterialKind, NativeVulkanSceneMaterialPass,
    NativeVulkanSceneSampledImageEffectPass, NativeVulkanSceneTextureSlot,
};

mod drift;
mod flutter;
mod iris;
pub(super) mod motion;
mod opacity_mask;
mod sway_shake;
mod utility;
mod water_caustics;
mod water_flow;
mod water_ripple;
mod water_waves;

pub(super) fn native_vulkan_scene_effect_passes_from_render_passes(
    passes: &[SceneRenderImageEffectPass],
) -> Vec<NativeVulkanSceneEffectRecord> {
    passes
        .iter()
        .map(|pass| {
            let semantics =
                native_vulkan_scene_effect_semantics(pass.runtime.as_deref(), &pass.effect_file);
            NativeVulkanSceneEffectRecord {
                kind: semantics.kind,
                evaluation_boundary: semantics.evaluation_boundary,
                effect_file: pass.effect_file.clone(),
                runtime: pass.runtime.clone(),
                pass_index: pass.pass_index,
                shader: pass.shader.clone(),
                blending: pass.blending.clone(),
                texture_slots: pass
                    .texture_slots
                    .iter()
                    .map(|slot| NativeVulkanSceneTextureSlot {
                        slot: slot.slot,
                        source: slot.source.clone(),
                        width: slot.width,
                        height: slot.height,
                    })
                    .collect(),
                parameter_keys: pass.constant_shader_values.keys().cloned().collect(),
                combo_keys: pass.combos.keys().cloned().collect(),
                depth_test: native_vulkan_scene_material_flag_from_optional(
                    pass.depthtest.as_deref(),
                ),
                depth_write: native_vulkan_scene_material_flag_from_optional(
                    pass.depthwrite.as_deref(),
                ),
                cull_mode: native_vulkan_scene_cull_mode_from_optional(pass.cullmode.as_deref()),
            }
        })
        .collect()
}

pub(super) fn native_vulkan_scene_effect_passes_from_scene_passes(
    passes: &[SceneImageEffectPass],
) -> Vec<NativeVulkanSceneEffectRecord> {
    passes
        .iter()
        .map(|pass| {
            let semantics =
                native_vulkan_scene_effect_semantics(pass.runtime.as_deref(), &pass.effect_file);
            NativeVulkanSceneEffectRecord {
                kind: semantics.kind,
                evaluation_boundary: semantics.evaluation_boundary,
                effect_file: pass.effect_file.clone(),
                runtime: pass.runtime.clone(),
                pass_index: pass.pass_index,
                shader: pass.shader.clone(),
                blending: pass.blending.clone(),
                texture_slots: native_vulkan_scene_texture_slots_from_scene_slots(
                    &pass.texture_slots,
                ),
                parameter_keys: pass.constant_shader_values.keys().cloned().collect(),
                combo_keys: pass.combos.keys().cloned().collect(),
                depth_test: native_vulkan_scene_material_flag_from_optional(
                    pass.depthtest.as_deref(),
                ),
                depth_write: native_vulkan_scene_material_flag_from_optional(
                    pass.depthwrite.as_deref(),
                ),
                cull_mode: native_vulkan_scene_cull_mode_from_optional(pass.cullmode.as_deref()),
            }
        })
        .collect()
}

pub(super) fn native_vulkan_scene_sampled_image_material_pass(
    kind: NativeVulkanSceneMaterialKind,
    blend_mode: SceneBlendMode,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneRenderAlphaTextureMode,
    texture_slot_count: usize,
    effect_passes: &[NativeVulkanSceneEffectRecord],
) -> NativeVulkanSceneMaterialPass {
    let material_source = effect_passes.first();
    let render_state = native_vulkan_scene_render_state(
        blend_mode,
        material_source
            .map(|pass| pass.depth_test)
            .unwrap_or(NativeVulkanSceneMaterialFlag::Unspecified),
        material_source
            .map(|pass| pass.depth_write)
            .unwrap_or(NativeVulkanSceneMaterialFlag::Unspecified),
        material_source
            .map(|pass| pass.cull_mode.clone())
            .unwrap_or(NativeVulkanSceneCullMode::Unspecified),
    );
    NativeVulkanSceneMaterialPass {
        kind,
        shader: material_source.and_then(|pass| pass.shader.clone()),
        blending: material_source.and_then(|pass| pass.blending.clone()),
        render_state,
        alpha_texture_slot,
        alpha_texture_mode,
        texture_slot_count,
        effect_kinds: native_vulkan_scene_effect_kind_list(effect_passes),
        combo_keys: native_vulkan_scene_effect_combo_key_list(effect_passes),
    }
}

pub(super) fn native_vulkan_scene_render_first_class_effect_target_pass(
    passes: &[SceneRenderImageEffectPass],
) -> Option<NativeVulkanSceneSampledImageEffectPass> {
    passes
        .iter()
        .find(|pass| iris::uses_first_class_target(pass.runtime.as_deref(), &pass.effect_file))
        .and_then(|pass| {
            native_vulkan_scene_first_class_effect_target_pass_from_slots(
                native_vulkan_scene_texture_slots_from_render_slots(&pass.texture_slots),
                pass.runtime.as_deref(),
                &pass.effect_file,
            )
        })
}

pub(super) fn native_vulkan_scene_snapshot_first_class_effect_target_pass(
    passes: &[SceneImageEffectPass],
) -> Option<NativeVulkanSceneSampledImageEffectPass> {
    passes
        .iter()
        .find(|pass| iris::uses_first_class_target(pass.runtime.as_deref(), &pass.effect_file))
        .and_then(|pass| {
            native_vulkan_scene_first_class_effect_target_pass_from_slots(
                native_vulkan_scene_texture_slots_from_scene_slots(&pass.texture_slots),
                pass.runtime.as_deref(),
                &pass.effect_file,
            )
        })
}

pub(super) fn native_vulkan_scene_effect_records_label(
    records: &[NativeVulkanSceneEffectRecord],
) -> String {
    if records.is_empty() {
        return "[]".to_owned();
    }
    let mut label = String::from("[");
    for (index, record) in records.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(record.kind.as_str());
        label.push('#');
        label.push_str(&record.pass_index.to_string());
        label.push_str(":shader=");
        label.push_str(record.shader.as_deref().unwrap_or("<none>"));
        label.push_str(":blend=");
        label.push_str(record.blending.as_deref().unwrap_or("<none>"));
        label.push_str(":boundary=");
        label.push_str(record.evaluation_boundary.as_str());
    }
    label.push(']');
    label
}

pub(super) fn native_vulkan_scene_material_pass_label(
    material: &NativeVulkanSceneMaterialPass,
) -> String {
    format!(
        "kind={} shader={} blending={} blend={:?} equation={} alpha_slot={:?} alpha_mode={} depth_test={} depth_write={} cull={} texture_slots={} effect_kinds={} pipeline={}",
        material.kind.as_str(),
        material.shader.as_deref().unwrap_or("<none>"),
        material.blending.as_deref().unwrap_or("<none>"),
        material.render_state.blend.mode,
        native_vulkan_scene_blend_equation_label(material.render_state.blend),
        material.alpha_texture_slot,
        material.alpha_texture_mode.as_str(),
        material.render_state.depth_test.as_str(),
        material.render_state.depth_write.as_str(),
        material.render_state.cull_mode.label(),
        material.texture_slot_count,
        native_vulkan_scene_effect_kind_label(&material.effect_kinds),
        native_vulkan_scene_sampled_image_pipeline_label(&material.render_state),
    )
}

fn native_vulkan_scene_effect_kind(
    runtime: Option<&str>,
    effect_file: &str,
) -> NativeVulkanSceneEffectKind {
    native_vulkan_scene_effect_semantics(runtime, effect_file).kind
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanSceneEffectSemantics {
    kind: NativeVulkanSceneEffectKind,
    evaluation_boundary: NativeVulkanSceneEffectEvaluationBoundary,
}

fn native_vulkan_scene_effect_semantics(
    runtime: Option<&str>,
    effect_file: &str,
) -> NativeVulkanSceneEffectSemantics {
    let file = native_vulkan_scene_normalized_effect_file(effect_file);
    let kind = if opacity_mask::matches(runtime, &file) {
        NativeVulkanSceneEffectKind::OpacityMask
    } else if iris::matches(runtime, &file) {
        NativeVulkanSceneEffectKind::Iris
    } else if water_caustics::matches(runtime, &file) {
        NativeVulkanSceneEffectKind::WaterCaustics
    } else if water_ripple::matches(&file) {
        NativeVulkanSceneEffectKind::WaterRipple
    } else if water_waves::matches(&file) {
        NativeVulkanSceneEffectKind::WaterWaves
    } else if water_flow::matches(&file) {
        NativeVulkanSceneEffectKind::WaterFlow
    } else if let Some(kind) = motion::classify(&file) {
        kind
    } else if let Some(kind) = utility::classify(&file) {
        kind
    } else {
        NativeVulkanSceneEffectKind::ShaderMaterial
    };
    NativeVulkanSceneEffectSemantics {
        kind,
        evaluation_boundary: native_vulkan_scene_effect_evaluation_boundary(kind),
    }
}

fn native_vulkan_scene_effect_evaluation_boundary(
    kind: NativeVulkanSceneEffectKind,
) -> NativeVulkanSceneEffectEvaluationBoundary {
    match kind {
        NativeVulkanSceneEffectKind::Iris => {
            NativeVulkanSceneEffectEvaluationBoundary::FirstClassTarget
        }
        NativeVulkanSceneEffectKind::SwayShake => {
            NativeVulkanSceneEffectEvaluationBoundary::FinalFrameTransform
        }
        NativeVulkanSceneEffectKind::Flutter => flutter::evaluation_boundary(),
        NativeVulkanSceneEffectKind::Drift => drift::evaluation_boundary(),
        NativeVulkanSceneEffectKind::Blur | NativeVulkanSceneEffectKind::CompositeLayer => {
            NativeVulkanSceneEffectEvaluationBoundary::UtilityPass
        }
        NativeVulkanSceneEffectKind::OpacityMask
        | NativeVulkanSceneEffectKind::WaterRipple
        | NativeVulkanSceneEffectKind::WaterWaves
        | NativeVulkanSceneEffectKind::WaterFlow
        | NativeVulkanSceneEffectKind::WaterCaustics
        | NativeVulkanSceneEffectKind::UserBindings
        | NativeVulkanSceneEffectKind::ShaderMaterial => {
            NativeVulkanSceneEffectEvaluationBoundary::MaterialPass
        }
    }
}

fn native_vulkan_scene_normalized_effect_file(effect_file: &str) -> String {
    effect_file.replace('\\', "/").to_ascii_lowercase()
}

fn native_vulkan_scene_first_class_effect_target_pass_from_slots(
    texture_slots: Vec<NativeVulkanSceneTextureSlot>,
    runtime: Option<&str>,
    effect_file: &str,
) -> Option<NativeVulkanSceneSampledImageEffectPass> {
    let normalized = native_vulkan_scene_normalized_effect_file(effect_file);
    if !iris::matches(runtime, &normalized) {
        return None;
    }
    let alpha_texture_slot = iris::alpha_texture_slot(&texture_slots)?;
    Some(NativeVulkanSceneSampledImageEffectPass {
        texture_slots,
        alpha_texture_slot: Some(alpha_texture_slot),
        alpha_texture_mode: SceneRenderAlphaTextureMode::Iris,
    })
}

fn native_vulkan_scene_effect_kind_list(
    passes: &[NativeVulkanSceneEffectRecord],
) -> Vec<NativeVulkanSceneEffectKind> {
    let mut kinds = Vec::new();
    for pass in passes {
        if !kinds.contains(&pass.kind) {
            kinds.push(pass.kind);
        }
    }
    kinds
}

fn native_vulkan_scene_effect_combo_key_list(
    passes: &[NativeVulkanSceneEffectRecord],
) -> Vec<String> {
    let mut keys = Vec::new();
    for pass in passes {
        for key in &pass.combo_keys {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    keys
}

fn native_vulkan_scene_effect_kind_label(kinds: &[NativeVulkanSceneEffectKind]) -> String {
    if kinds.is_empty() {
        return "[]".to_owned();
    }
    let mut label = String::from("[");
    for (index, kind) in kinds.iter().enumerate() {
        if index > 0 {
            label.push(',');
        }
        label.push_str(kind.as_str());
    }
    label.push(']');
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_family_modules_classify_core_native_effects() {
        let cases = [
            (
                Some("native-opacity-mask"),
                "effects/anything/effect.json",
                NativeVulkanSceneEffectKind::OpacityMask,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
            (
                Some("native-iris-mask"),
                "effects/anything/effect.json",
                NativeVulkanSceneEffectKind::Iris,
                NativeVulkanSceneEffectEvaluationBoundary::FirstClassTarget,
            ),
            (
                Some("native-water-caustics"),
                "effects/anything/effect.json",
                NativeVulkanSceneEffectKind::WaterCaustics,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
            (
                None,
                "effects/waterripple/effect.json",
                NativeVulkanSceneEffectKind::WaterRipple,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
            (
                None,
                "effects/water_waves/effect.json",
                NativeVulkanSceneEffectKind::WaterWaves,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
            (
                None,
                "effects/waterflow/effect.json",
                NativeVulkanSceneEffectKind::WaterFlow,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
            (
                None,
                "effects/flutter/effect.json",
                NativeVulkanSceneEffectKind::Flutter,
                NativeVulkanSceneEffectEvaluationBoundary::FinalFrameVertex,
            ),
            (
                None,
                "effects/drift/effect.json",
                NativeVulkanSceneEffectKind::Drift,
                NativeVulkanSceneEffectEvaluationBoundary::FinalFrameVertex,
            ),
            (
                None,
                "effects/sway/effect.json",
                NativeVulkanSceneEffectKind::SwayShake,
                NativeVulkanSceneEffectEvaluationBoundary::FinalFrameTransform,
            ),
            (
                None,
                "effects/fullscreenlayer/effect.json",
                NativeVulkanSceneEffectKind::CompositeLayer,
                NativeVulkanSceneEffectEvaluationBoundary::UtilityPass,
            ),
            (
                None,
                "effects/newproperty5/effect.json",
                NativeVulkanSceneEffectKind::UserBindings,
                NativeVulkanSceneEffectEvaluationBoundary::MaterialPass,
            ),
        ];
        for (runtime, effect_file, expected_kind, expected_boundary) in cases {
            let semantics = native_vulkan_scene_effect_semantics(runtime, effect_file);
            assert_eq!(semantics.kind, expected_kind, "{effect_file}");
            assert_eq!(
                semantics.evaluation_boundary, expected_boundary,
                "{effect_file}"
            );
        }
    }

    #[test]
    fn iris_family_owns_first_class_target_policy() {
        let mut iris_pass = SceneRenderImageEffectPass {
            effect_file: "materials/effects/iris/effect.json".to_owned(),
            runtime: None,
            pass_index: 0,
            shader: Some("effects/iris".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            texture_slots: vec![
                SceneRenderTextureSlot {
                    slot: 2,
                    source: std::path::PathBuf::from("textures/iris-mask-b.gtex"),
                    width: Some(16),
                    height: Some(16),
                },
                SceneRenderTextureSlot {
                    slot: 1,
                    source: std::path::PathBuf::from("textures/iris-mask-a.gtex"),
                    width: Some(16),
                    height: Some(16),
                },
            ],
            combos: Default::default(),
            constant_shader_values: Default::default(),
        };

        let target =
            native_vulkan_scene_render_first_class_effect_target_pass(&[iris_pass.clone()])
                .expect("iris effect should own a first-class target pass");
        assert_eq!(target.alpha_texture_slot, Some(1));
        assert_eq!(target.alpha_texture_mode, SceneRenderAlphaTextureMode::Iris);
        assert_eq!(target.texture_slots[0].slot, 1);
        assert_eq!(target.texture_slots[1].slot, 2);

        iris_pass.runtime = Some("native-iris-mask".to_owned());
        iris_pass.effect_file = "effects/other/effect.json".to_owned();
        assert!(native_vulkan_scene_render_first_class_effect_target_pass(&[iris_pass]).is_some());

        let opacity_pass = SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("native-opacity-mask".to_owned()),
            pass_index: 0,
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: std::path::PathBuf::from("textures/opacity-mask.gtex"),
                width: Some(16),
                height: Some(16),
            }],
            combos: Default::default(),
            constant_shader_values: Default::default(),
        };
        assert!(
            native_vulkan_scene_render_first_class_effect_target_pass(&[opacity_pass]).is_none()
        );
    }
}

fn native_vulkan_scene_material_flag_from_optional(
    value: Option<&str>,
) -> NativeVulkanSceneMaterialFlag {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "1" | "true" | "enabled" | "enable" | "on") => {
            NativeVulkanSceneMaterialFlag::Enabled
        }
        Some(value)
            if matches!(
                value.as_str(),
                "0" | "false" | "disabled" | "disable" | "off"
            ) =>
        {
            NativeVulkanSceneMaterialFlag::Disabled
        }
        Some(_) => NativeVulkanSceneMaterialFlag::Unspecified,
        None => NativeVulkanSceneMaterialFlag::Unspecified,
    }
}

fn native_vulkan_scene_cull_mode_from_optional(value: Option<&str>) -> NativeVulkanSceneCullMode {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "none" | "off" | "disabled" | "disable") => {
            NativeVulkanSceneCullMode::None
        }
        Some(value) if value == "back" => NativeVulkanSceneCullMode::Back,
        Some(value) if value == "front" => NativeVulkanSceneCullMode::Front,
        Some(value) if matches!(value.as_str(), "frontandback" | "front-and-back") => {
            NativeVulkanSceneCullMode::FrontAndBack
        }
        Some(value) if value.is_empty() => NativeVulkanSceneCullMode::Unspecified,
        Some(value) => NativeVulkanSceneCullMode::Named(value),
        None => NativeVulkanSceneCullMode::Unspecified,
    }
}

fn native_vulkan_scene_texture_slots_from_scene_slots(
    slots: &[SceneTextureSlot],
) -> Vec<NativeVulkanSceneTextureSlot> {
    let mut output = slots
        .iter()
        .map(|slot| NativeVulkanSceneTextureSlot {
            slot: slot.slot,
            source: PathBuf::from(slot.source.as_str()),
            width: slot.width,
            height: slot.height,
        })
        .collect::<Vec<_>>();
    output.sort_by_key(|slot| slot.slot);
    output.dedup_by(|left, right| left.slot == right.slot && left.source == right.source);
    output
}

fn native_vulkan_scene_texture_slots_from_render_slots(
    slots: &[SceneRenderTextureSlot],
) -> Vec<NativeVulkanSceneTextureSlot> {
    let mut output = slots
        .iter()
        .map(|slot| NativeVulkanSceneTextureSlot {
            slot: slot.slot,
            source: slot.source.clone(),
            width: slot.width,
            height: slot.height,
        })
        .collect::<Vec<_>>();
    output.sort_by_key(|slot| slot.slot);
    output.dedup_by(|left, right| left.slot == right.slot && left.source == right.source);
    output
}

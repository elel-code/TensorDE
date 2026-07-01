use std::path::PathBuf;

use crate::core::SceneBlendMode;
use crate::core::scene::{SceneImageEffectPass, SceneTextureSlot};
use crate::renderer::{SceneRenderAlphaTextureMode, SceneRenderImageEffectPass};

use super::blend::{
    native_vulkan_scene_render_state, native_vulkan_scene_sampled_image_pipeline_label,
};
use super::{
    NativeVulkanSceneCullMode, NativeVulkanSceneEffectKind, NativeVulkanSceneEffectRecord,
    NativeVulkanSceneMaterialFlag, NativeVulkanSceneMaterialKind, NativeVulkanSceneMaterialPass,
    NativeVulkanSceneTextureSlot,
};

pub(super) fn native_vulkan_scene_effect_passes_from_render_passes(
    passes: &[SceneRenderImageEffectPass],
) -> Vec<NativeVulkanSceneEffectRecord> {
    passes
        .iter()
        .map(|pass| NativeVulkanSceneEffectRecord {
            kind: native_vulkan_scene_effect_kind(pass.runtime.as_deref(), &pass.effect_file),
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
            depth_test: native_vulkan_scene_material_flag_from_optional(pass.depthtest.as_deref()),
            depth_write: native_vulkan_scene_material_flag_from_optional(
                pass.depthwrite.as_deref(),
            ),
            cull_mode: native_vulkan_scene_cull_mode_from_optional(pass.cullmode.as_deref()),
        })
        .collect()
}

pub(super) fn native_vulkan_scene_effect_passes_from_scene_passes(
    passes: &[SceneImageEffectPass],
) -> Vec<NativeVulkanSceneEffectRecord> {
    passes
        .iter()
        .map(|pass| NativeVulkanSceneEffectRecord {
            kind: native_vulkan_scene_effect_kind(pass.runtime.as_deref(), &pass.effect_file),
            effect_file: pass.effect_file.clone(),
            runtime: pass.runtime.clone(),
            pass_index: pass.pass_index,
            shader: pass.shader.clone(),
            blending: pass.blending.clone(),
            texture_slots: native_vulkan_scene_texture_slots_from_scene_slots(&pass.texture_slots),
            parameter_keys: pass.constant_shader_values.keys().cloned().collect(),
            combo_keys: pass.combos.keys().cloned().collect(),
            depth_test: native_vulkan_scene_material_flag_from_optional(pass.depthtest.as_deref()),
            depth_write: native_vulkan_scene_material_flag_from_optional(
                pass.depthwrite.as_deref(),
            ),
            cull_mode: native_vulkan_scene_cull_mode_from_optional(pass.cullmode.as_deref()),
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

pub(super) fn native_vulkan_scene_effect_pass_uses_first_class_target(
    runtime: Option<&str>,
    effect_file: &str,
) -> bool {
    matches!(
        native_vulkan_scene_effect_kind(runtime, effect_file),
        NativeVulkanSceneEffectKind::Iris
    )
}

pub(super) fn native_vulkan_scene_effect_pass_is_iris(
    runtime: Option<&str>,
    effect_file: &str,
) -> bool {
    matches!(
        native_vulkan_scene_effect_kind(runtime, effect_file),
        NativeVulkanSceneEffectKind::Iris
    )
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
    }
    label.push(']');
    label
}

pub(super) fn native_vulkan_scene_material_pass_label(
    material: &NativeVulkanSceneMaterialPass,
) -> String {
    format!(
        "kind={} shader={} blending={} blend={:?} alpha_slot={:?} alpha_mode={} depth_test={} depth_write={} cull={} texture_slots={} effect_kinds={} pipeline={}",
        material.kind.as_str(),
        material.shader.as_deref().unwrap_or("<none>"),
        material.blending.as_deref().unwrap_or("<none>"),
        material.render_state.blend.mode,
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
    match runtime {
        Some("native-opacity-mask") => return NativeVulkanSceneEffectKind::OpacityMask,
        Some("native-iris-mask") => return NativeVulkanSceneEffectKind::Iris,
        Some("native-water-caustics") => return NativeVulkanSceneEffectKind::WaterCaustics,
        _ => {}
    }

    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    if file.contains("opacity") {
        NativeVulkanSceneEffectKind::OpacityMask
    } else if file.contains("iris") {
        NativeVulkanSceneEffectKind::Iris
    } else if file.contains("waterripple") || file.contains("water_ripple") {
        NativeVulkanSceneEffectKind::WaterRipple
    } else if file.contains("waterwaves") || file.contains("water_waves") {
        NativeVulkanSceneEffectKind::WaterWaves
    } else if file.contains("waterflow") || file.contains("water_flow") {
        NativeVulkanSceneEffectKind::WaterFlow
    } else if file.contains("watercaustics") || file.contains("water_caustics") {
        NativeVulkanSceneEffectKind::WaterCaustics
    } else if file.contains("blur") {
        NativeVulkanSceneEffectKind::Blur
    } else if file.contains("sway") || file.contains("shake") {
        NativeVulkanSceneEffectKind::SwayShake
    } else if file.contains("flutter") {
        NativeVulkanSceneEffectKind::Flutter
    } else if file.contains("drift") {
        NativeVulkanSceneEffectKind::Drift
    } else if file.contains("composelayer") || file.contains("fullscreenlayer") {
        NativeVulkanSceneEffectKind::CompositeLayer
    } else if file.contains("newproperty") || file.contains("user") {
        NativeVulkanSceneEffectKind::UserBindings
    } else {
        NativeVulkanSceneEffectKind::ShaderMaterial
    }
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

//! Effect descriptor-heap heap-slice keys.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`

use std::collections::BTreeSet;

use serde::Serialize;

use crate::engine::scene_engine::{
    SCENE_WE_MAX_SHADER_TEXTURE_SLOTS, SceneEffectPassGraphInputSource,
    SceneEffectPassGraphMaterialPass, SceneGraphResourceRole,
};

use super::super::effect_descriptors::{
    NativeVulkanSceneEffectTextureDescriptorBinding,
    NativeVulkanSceneEffectTextureDescriptorFramePlan,
};
use super::super::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTextureSetKey {
    pub bindings: Vec<NativeVulkanSceneEffectTextureSetBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTextureSetBinding {
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub source: NativeVulkanSceneTextureDescriptorSource,
}

pub(super) fn effect_texture_descriptors_by_pass(
    descriptors: &NativeVulkanSceneEffectTextureDescriptorFramePlan,
) -> Result<Vec<Vec<&NativeVulkanSceneEffectTextureDescriptorBinding>>, String> {
    let mut by_pass = vec![Vec::new(); descriptors.pass_count];
    for descriptor in &descriptors.bindings {
        let pass = by_pass
            .get_mut(descriptor.effect_pass_index)
            .ok_or_else(|| {
                format!(
                    "scene effect resource heap descriptor pass index {} exceeds pass count {}",
                    descriptor.effect_pass_index, descriptors.pass_count
                )
            })?;
        pass.push(descriptor);
    }
    for pass in &mut by_pass {
        pass.sort_by_key(|descriptor| descriptor.slot);
    }
    Ok(by_pass)
}

pub(super) fn effect_texture_set_key(
    descriptors: &[&NativeVulkanSceneEffectTextureDescriptorBinding],
) -> NativeVulkanSceneEffectTextureSetKey {
    let bindings = descriptors
        .iter()
        .map(|descriptor| NativeVulkanSceneEffectTextureSetBinding {
            slot: descriptor.slot,
            role: descriptor.role,
            source: descriptor.source,
        })
        .collect::<Vec<_>>();
    NativeVulkanSceneEffectTextureSetKey { bindings }
}

pub(super) fn effect_pass_texture_set_key(
    pass: &SceneEffectPassGraphMaterialPass,
) -> Result<NativeVulkanSceneEffectTextureSetKey, String> {
    let mut used_slots = BTreeSet::new();
    let mut bindings = Vec::new();
    if let Some(source) = pass.source.as_ref() {
        push_effect_pass_texture_set_binding(
            pass,
            &mut used_slots,
            &mut bindings,
            source.slot,
            &source.source,
        )?;
    }
    for input in &pass.input_bindings {
        push_effect_pass_texture_set_binding(
            pass,
            &mut used_slots,
            &mut bindings,
            input.slot,
            &input.source,
        )?;
    }
    for resource in &pass.texture_resources {
        push_effect_texture_set_slot(pass, &mut used_slots, resource.slot)?;
        bindings.push(NativeVulkanSceneEffectTextureSetBinding {
            slot: resource.slot,
            role: SceneGraphResourceRole::shader_texture(resource.slot),
            source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(resource.resource),
        });
    }
    bindings.sort_by_key(|binding| binding.slot);
    Ok(NativeVulkanSceneEffectTextureSetKey { bindings })
}

fn push_effect_pass_texture_set_binding(
    pass: &SceneEffectPassGraphMaterialPass,
    used_slots: &mut BTreeSet<u32>,
    bindings: &mut Vec<NativeVulkanSceneEffectTextureSetBinding>,
    slot: u32,
    source: &SceneEffectPassGraphInputSource,
) -> Result<(), String> {
    push_effect_texture_set_slot(pass, used_slots, slot)?;
    bindings.push(NativeVulkanSceneEffectTextureSetBinding {
        slot,
        role: SceneGraphResourceRole::shader_texture(slot),
        source: effect_pass_input_descriptor_source(pass, source),
    });
    Ok(())
}

fn push_effect_texture_set_slot(
    pass: &SceneEffectPassGraphMaterialPass,
    used_slots: &mut BTreeSet<u32>,
    slot: u32,
) -> Result<(), String> {
    if slot >= SCENE_WE_MAX_SHADER_TEXTURE_SLOTS {
        return Err(format!(
            "scene effect pass {} for object {:?} texture slot {} exceeds WE slot mask width {}",
            pass.pass_index, pass.object, slot, SCENE_WE_MAX_SHADER_TEXTURE_SLOTS
        ));
    }
    if !used_slots.insert(slot) {
        return Err(format!(
            "scene effect pass {} for object {:?} binds texture slot {} more than once",
            pass.pass_index, pass.object, slot
        ));
    }
    Ok(())
}

fn effect_pass_input_descriptor_source(
    pass: &SceneEffectPassGraphMaterialPass,
    source: &SceneEffectPassGraphInputSource,
) -> NativeVulkanSceneTextureDescriptorSource {
    match *source {
        SceneEffectPassGraphInputSource::ObjectSourceTexture(resource) => {
            NativeVulkanSceneTextureDescriptorSource::ResidentTexture(resource)
        }
        SceneEffectPassGraphInputSource::GraphTarget(target) => {
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(target)
        }
        SceneEffectPassGraphInputSource::PreviousFramebuffer => {
            NativeVulkanSceneTextureDescriptorSource::PreviousFramebuffer {
                object: pass.object,
                effect_pass_index: pass.graph_pass_index,
            }
        }
        SceneEffectPassGraphInputSource::Scene => NativeVulkanSceneTextureDescriptorSource::Scene {
            object: pass.object,
            effect_pass_index: pass.graph_pass_index,
        },
    }
}

pub(super) fn effect_heap_slice_shader_mappings(
    texture_set: &NativeVulkanSceneEffectTextureSetKey,
) -> Vec<String> {
    texture_set
        .bindings
        .iter()
        .enumerate()
        .map(|(ordinal, binding)| {
            format!(
                "{} -> effect-heap-slice-offset{}",
                binding_shader_mapping(binding.slot),
                ordinal
            )
        })
        .collect()
}

pub(super) fn binding_shader_mapping(slot: u32) -> String {
    format!("set0.binding{slot}.g_Texture{slot}")
}

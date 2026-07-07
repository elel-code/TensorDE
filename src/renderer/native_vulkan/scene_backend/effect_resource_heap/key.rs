//! Effect descriptor-heap resource-set keys.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`

use serde::Serialize;

use crate::engine::scene_engine::SceneGraphResourceRole;

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

pub(super) fn effect_resource_set_shader_mappings(
    texture_set: &NativeVulkanSceneEffectTextureSetKey,
) -> Vec<String> {
    texture_set
        .bindings
        .iter()
        .enumerate()
        .map(|(ordinal, binding)| {
            format!(
                "{} -> effect-resource-set-offset{}",
                binding_shader_mapping(binding.slot),
                ordinal
            )
        })
        .collect()
}

pub(super) fn binding_shader_mapping(slot: u32) -> String {
    format!("set0.binding{slot}.g_Texture{slot}")
}

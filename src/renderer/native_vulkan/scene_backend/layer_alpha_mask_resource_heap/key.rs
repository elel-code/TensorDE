//! WE layer alpha-mask descriptor heap slice keys.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`

use std::collections::BTreeSet;

use serde::Serialize;

use crate::engine::scene_engine::SCENE_WE_MAX_SHADER_TEXTURE_SLOTS;

use super::super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskDescriptorPlan, NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskSlotBinding, NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
    pub shader: String,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskHeapSliceBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
    pub slot: u32,
    pub source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
}

pub(super) fn alpha_mask_slots_by_texture_bind(
    descriptors: &NativeVulkanSceneLayerAlphaMaskDescriptorPlan,
) -> Result<Vec<Vec<NativeVulkanSceneLayerAlphaMaskSlotBinding>>, String> {
    let mut by_heap_bind = Vec::with_capacity(descriptors.entries.len());
    for texture_bind in &descriptors.entries {
        let mut slots = texture_bind.slots.clone();
        slots.sort_by_key(|slot| slot.slot);
        validate_alpha_mask_slots(texture_bind, &slots)?;
        by_heap_bind.push(slots);
    }
    Ok(by_heap_bind)
}

pub(super) fn alpha_mask_texture_bind_heap_slice(
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskHeapSliceKey, String> {
    let mut slots = texture_bind.slots.clone();
    slots.sort_by_key(|slot| slot.slot);
    validate_alpha_mask_slots(texture_bind, &slots)?;
    let bindings = slots
        .iter()
        .map(|slot| NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
            slot: slot.slot,
            source: slot.source,
        })
        .collect::<Vec<_>>();
    Ok(NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
        shader: texture_bind.shader.to_owned(),
        bindings,
    })
}

pub(super) fn alpha_mask_heap_slice_shader_mappings(
    heap_slice: &NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    has_material_uniform: bool,
) -> Vec<String> {
    let mut mappings = Vec::new();
    if has_material_uniform {
        mappings
            .push("WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned());
    }
    let texture_offset = usize::from(has_material_uniform);
    mappings.extend(
        heap_slice
            .bindings
            .iter()
            .enumerate()
            .map(|(ordinal, binding)| {
                format!(
                    "{} -> alpha-mask-heap-slice-offset{}",
                    binding_shader_mapping(binding.slot),
                    ordinal + texture_offset
                )
            }),
    );
    mappings
}

pub(super) fn binding_shader_mapping(slot: u32) -> String {
    format!("we.texture_slot{slot}.g_Texture{slot}")
}

fn validate_alpha_mask_slots(
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
    slots: &[NativeVulkanSceneLayerAlphaMaskSlotBinding],
) -> Result<(), String> {
    if texture_bind.shader.is_empty() {
        return Err(format!(
            "scene layer alpha-mask heap bind for object {:?} has empty shader",
            texture_bind.object
        ));
    }
    let mut used_slots = BTreeSet::new();
    for slot in slots {
        if slot.slot >= SCENE_WE_MAX_SHADER_TEXTURE_SLOTS {
            return Err(format!(
                "scene layer alpha-mask heap bind for object {:?} shader {} texture slot {} exceeds WE slot mask width {}",
                texture_bind.object,
                texture_bind.shader,
                slot.slot,
                SCENE_WE_MAX_SHADER_TEXTURE_SLOTS
            ));
        }
        if !used_slots.insert(slot.slot) {
            return Err(format!(
                "scene layer alpha-mask heap bind for object {:?} shader {} binds texture slot {} more than once",
                texture_bind.object, texture_bind.shader, slot.slot
            ));
        }
    }
    Ok(())
}

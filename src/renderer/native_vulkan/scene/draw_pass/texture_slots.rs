use std::path::PathBuf;

use crate::core::scene::SceneTextureSlot;
use crate::renderer::SceneRenderTextureSlot;

use super::{
    NativeVulkanSceneSampledImageQuad, NativeVulkanSceneTextureSlot,
    NativeVulkanSceneTextureSlotResourceBinding,
};

pub(super) fn native_vulkan_scene_sampled_image_source_index(
    sources: &mut Vec<PathBuf>,
    source: PathBuf,
) -> u32 {
    if let Some(index) = sources.iter().position(|existing| existing == &source) {
        return index.min(u32::MAX as usize) as u32;
    }
    let index = sources.len().min(u32::MAX as usize) as u32;
    sources.push(source);
    index
}

pub(super) fn native_vulkan_scene_texture_slots_from_render_slots(
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

pub(super) fn native_vulkan_scene_texture_slots_from_scene_slots(
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

pub(super) fn native_vulkan_scene_sampled_image_texture_slot_bindings(
    sources: &mut Vec<PathBuf>,
    quad: &NativeVulkanSceneSampledImageQuad,
    base_resource_index: u32,
) -> Vec<NativeVulkanSceneTextureSlotResourceBinding> {
    native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots(
        sources,
        &quad.texture_slots,
        base_resource_index,
    )
}

pub(super) fn native_vulkan_scene_sampled_image_texture_slot_bindings_for_slots(
    sources: &mut Vec<PathBuf>,
    texture_slots: &[NativeVulkanSceneTextureSlot],
    base_resource_index: u32,
) -> Vec<NativeVulkanSceneTextureSlotResourceBinding> {
    let mut resources = vec![base_resource_index];
    for slot in texture_slots {
        let Ok(slot_index) = usize::try_from(slot.slot) else {
            continue;
        };
        if slot_index == 0 {
            continue;
        }
        if resources.len() <= slot_index {
            resources.resize(slot_index + 1, base_resource_index);
        }
        resources[slot_index] =
            native_vulkan_scene_sampled_image_source_index(sources, slot.source.clone());
    }
    resources
        .into_iter()
        .enumerate()
        .map(
            |(slot, resource_index)| NativeVulkanSceneTextureSlotResourceBinding {
                slot: slot.min(u32::MAX as usize) as u32,
                resource_index,
            },
        )
        .collect()
}

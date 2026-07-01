use std::path::PathBuf;

use crate::core::scene::SceneTextureSlot;
use crate::renderer::SceneRenderTextureSlot;

use super::{NativeVulkanSceneSampledImageQuad, NativeVulkanSceneTextureSlot};

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

pub(super) fn native_vulkan_scene_sampled_image_texture_slot_resource_indices(
    sources: &mut Vec<PathBuf>,
    quad: &NativeVulkanSceneSampledImageQuad,
    base_resource_index: u32,
) -> Vec<u32> {
    native_vulkan_scene_sampled_image_texture_slot_resource_indices_for_slots(
        sources,
        &quad.texture_slots,
        base_resource_index,
    )
}

pub(super) fn native_vulkan_scene_sampled_image_texture_slot_resource_indices_for_slots(
    sources: &mut Vec<PathBuf>,
    texture_slots: &[NativeVulkanSceneTextureSlot],
    base_resource_index: u32,
) -> Vec<u32> {
    let mut indices = vec![base_resource_index];
    for slot in texture_slots {
        let Ok(slot_index) = usize::try_from(slot.slot) else {
            continue;
        };
        if slot_index == 0 {
            continue;
        }
        if indices.len() <= slot_index {
            indices.resize(slot_index + 1, base_resource_index);
        }
        indices[slot_index] =
            native_vulkan_scene_sampled_image_source_index(sources, slot.source.clone());
    }
    indices
}

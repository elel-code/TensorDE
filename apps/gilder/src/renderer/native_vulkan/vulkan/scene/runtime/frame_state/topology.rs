//! Retained scene topology validation and skinning payload packing.

use std::fmt::Debug;

use crate::engine::scene::semantic_world::ResolvedSemanticFrame;
use crate::engine::scene::{
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw,
};
use crate::renderer::native_vulkan::NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;

pub(in super::super) fn pack_scene_skinning_palette(
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        graph
            .puppet_bone_matrices
            .len()
            .saturating_add(1)
            .saturating_mul(NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES),
    );
    push_scene_puppet_bone(
        &mut payload,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        1.0,
    );
    for bone in &graph.puppet_bone_matrices {
        push_scene_puppet_bone(&mut payload, bone.matrix, bone.alpha);
    }
    payload
}

pub(super) fn resolved_draw_effect_visibility_mask(
    frame: &ResolvedSemanticFrame,
    draw: &SceneRenderingDeviceMeshDraw,
) -> u32 {
    (0..draw.effect_binding_count.min(32)).fold(0u32, |mask, local_index| {
        if frame
            .object_effect(draw.effect_binding_start.saturating_add(local_index))
            .is_some_and(|effect| effect.resolved_visible)
        {
            mask | (1 << local_index)
        } else {
            mask
        }
    })
}

pub(super) fn validate_topology_slice<T: Debug + PartialEq>(
    role: &str,
    expected: &[T],
    actual: &[T],
    scene_time_seconds: f32,
) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "scene {role} topology changed at {scene_time_seconds:.6}s: setup count {}, frame count {}; live topology mutation is not supported by the current Vulkan resource allocation",
            expected.len(),
            actual.len()
        ));
    }
    if let Some((index, (expected, actual))) = expected
        .iter()
        .zip(actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
    {
        return Err(format!(
            "scene {role} topology changed at {scene_time_seconds:.6}s at index {index}: setup {expected:?}, frame {actual:?}; live topology mutation is not supported by the current Vulkan resource allocation"
        ));
    }
    Ok(())
}

fn push_scene_puppet_bone(payload: &mut Vec<u8>, matrix: [[f32; 4]; 4], alpha: f32) {
    for value in matrix.into_iter().flatten() {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    for value in [alpha, 0.0, 0.0, 0.0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
}

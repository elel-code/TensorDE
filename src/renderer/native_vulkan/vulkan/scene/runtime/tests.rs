use super::*;
use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
};

#[test]
fn automatic_surface_extent_prefers_authored_scene_pixels() {
    assert_eq!(
        scene_viewport::automatic_scene_surface_extent((3840, 2160), (2561, 1440)),
        (3840, 2160)
    );
    assert_eq!(
        scene_viewport::automatic_scene_surface_extent((0, 0), (2561, 1440)),
        (2561, 1440)
    );
}

#[test]
fn descriptor_plan_adds_skinning_storage_buffer_after_uniforms() {
    let draw = SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        clip_transform: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
        skinning_palette_start: 2,
        skinning_palette_count: 3,
        resolved_color: crate::engine::scene::SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        resolved_alpha: 1.0,
        apply_resolved_visual: true,
        effect_batch_atlas_tile: u32::MAX,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy:
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        instance_count: 1,
    };
    let layout = pipeline::ScenePipelineDescriptorLayout {
        sampled_slots: Vec::new(),
        material_uniform_enabled: true,
        skinning_storage_enabled: true,
    };

    let (descriptors, commands) = scene_descriptor_plan_inputs(
        &[draw],
        &[],
        &layout,
        &[2],
        &[None],
        &[Vec::new()],
    );

    assert_eq!(
        descriptors,
        vec![
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
        ]
    );
    assert_eq!(
        commands[0].skinning_byte_offset,
        3 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
    );
    assert_eq!(
        commands[0].skinning_byte_count,
        3 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
    );
    assert_eq!(commands[0].pipeline_index, 2);
}

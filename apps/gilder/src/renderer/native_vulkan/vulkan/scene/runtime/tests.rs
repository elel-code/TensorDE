use super::*;
use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
};

#[test]
fn automatic_surface_extent_uses_live_wayland_buffer_pixels() {
    assert_eq!(
        scene_viewport::scene_surface_extent(None, (2561, 1440)),
        (2561, 1440)
    );
}

#[test]
fn explicit_surface_extent_remains_a_deterministic_capture_override() {
    assert_eq!(
        scene_viewport::scene_surface_extent(Some((3856, 2199)), (2561, 1440)),
        (3856, 2199)
    );
}

#[test]
fn descriptor_plan_adds_skinning_storage_buffer_after_uniforms() {
    let draw = SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
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
        sampled_slots: vec![1, 3],
        input_attachment_slots: vec![7],
        material_uniform_enabled: true,
        skinning_storage_enabled: true,
    };

    let storage = crate::engine::scene::SceneStorage::from_document(
        crate::engine::scene::binary::SceneBinaryDocument::default(),
    )
    .expect("empty storage");
    let (descriptors, commands) = scene_descriptor_plan_inputs(
        &storage,
        &[draw],
        &[],
        &layout,
        &[2],
        &[None],
    );

    assert_eq!(
        descriptors,
        vec![
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
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
    assert_eq!(commands[0].resource_descriptor_base, 0);
    assert_eq!(commands[0].material_resource_descriptor, Some(1));
    assert_eq!(commands[0].skinning_resource_descriptor, Some(2));
    assert_eq!(commands[0].sampled_resource_descriptor_base, 3);
    assert_eq!(commands[0].input_attachment_resource_descriptor_base, 5);
}

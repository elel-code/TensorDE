use super::*;
use super::descriptor_plan::scene_descriptor_plan_inputs;
use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceMeshDraw,
};
use crate::renderer::rendering_device::RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;
use vulkan_renderer::DescriptorSlotKind;

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
    let draw = object_mesh_draw();
    let layout = descriptor_layout::ScenePipelineDescriptorLayout {
        sampled_slots: vec![1, 3],
        input_attachment_slots: vec![7],
        material_uniform_enabled: true,
        skinning_storage_enabled: true,
        particle_storage_enabled: false,
        scene_owned_uniform_count: 0,
    };

    let storage = crate::engine::scene::SceneStorage::from_document(
        crate::engine::scene::binary::SceneBinaryDocument::default(),
    )
    .expect("empty storage");
    let (descriptors, commands) =
        scene_descriptor_plan_inputs(&storage, &[draw], &[], &layout, &[2], &[None]);

    assert_eq!(
        descriptors,
        vec![
            SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND,
            SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND,
            DescriptorSlotKind::StorageBuffer,
            DescriptorSlotKind::SampledImage,
            DescriptorSlotKind::SampledImage,
            DescriptorSlotKind::InputAttachment,
        ]
    );
    assert_eq!(
        commands[0].skinning_byte_offset,
        3 * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
    );
    assert_eq!(
        commands[0].skinning_byte_count,
        3 * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64
    );
    assert_eq!(commands[0].pipeline_index, 2);
    assert_eq!(commands[0].resource_descriptor_base, 0);
    assert_eq!(commands[0].material_resource_descriptor, Some(1));
    assert_eq!(commands[0].skinning_resource_descriptor, Some(2));
    assert_eq!(commands[0].sampled_resource_descriptor_base, 3);
    assert_eq!(commands[0].input_attachment_resource_descriptor_base, 5);
    assert_eq!(commands[0].vertex_buffer_byte_offset, Some(0));
}

#[test]
fn scene_owned_fullscreen_vertex_reads_the_appended_utility_triangle() {
    let mesh_vertex_count = 37;
    let offset = descriptor_plan::vertex_buffer_byte_offset(
        mesh_vertex_count,
        12,
        11,
        0,
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        true,
    );

    assert_eq!(
        offset,
        Some((mesh_vertex_count + 11 * 3) as u64 * u64::from(SCENE_MESH_VERTEX_STRIDE_BYTES))
    );
    assert_eq!(
        descriptor_plan::vertex_buffer_byte_offset(
            mesh_vertex_count,
            12,
            11,
            0,
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            false,
        ),
        None,
        "engine-owned fullscreen vertices come from VertexIndex",
    );
}

#[test]
fn scene_owned_authored_quad_reads_six_vertices_after_fullscreen_utilities() {
    let mesh_vertex_count = 37;
    let fullscreen_utility_count = 11;
    let prior_quad_count = 2;

    assert_eq!(
        descriptor_plan::vertex_buffer_byte_offset(
            mesh_vertex_count,
            fullscreen_utility_count,
            0,
            prior_quad_count,
            SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad,
            true,
        ),
        Some(
            (mesh_vertex_count + fullscreen_utility_count * 3 + prior_quad_count * 6) as u64
                * u64::from(SCENE_MESH_VERTEX_STRIDE_BYTES)
        )
    );
}

#[test]
fn object_composite_vertex_offset_follows_mesh_and_fullscreen_payloads() {
    let mesh_vertex_count = 37;
    let fullscreen_utility_count = 11;
    let scene_owned_utility_quad_count = 2;
    let prior_composite_vertex_count = 8;

    assert_eq!(
        descriptor_plan::object_composite_vertex_buffer_byte_offset(
            mesh_vertex_count,
            fullscreen_utility_count,
            scene_owned_utility_quad_count,
            prior_composite_vertex_count,
        ),
        (mesh_vertex_count
            + fullscreen_utility_count * 3
            + scene_owned_utility_quad_count * 6
            + prior_composite_vertex_count) as u64
            * u64::from(SCENE_MESH_VERTEX_STRIDE_BYTES)
    );
}

#[test]
fn object_composite_command_binds_padded_vertices_and_keeps_index_range() {
    let vertex = crate::engine::scene::SceneMeshVertexRecord {
        position: crate::engine::scene::SceneVec3::default(),
        uv: [0.0; 2],
        blend_indices: [0; 4],
        blend_weights: [0.0; 4],
    };
    let storage = crate::engine::scene::SceneStorage::from_document(
        crate::engine::scene::binary::SceneBinaryDocument {
            mesh_vertices: vec![vertex; 37],
            ..crate::engine::scene::binary::SceneBinaryDocument::default()
        },
    )
    .expect("storage with retained mesh payload");
    let mut draw = object_mesh_draw();
    draw.vertex_start = 12;
    draw.index_start = 19;
    draw.index_count = 6;
    draw.uv_inset_texels = 0.15;
    draw.authored_source_extent = [2_560.0, 1_152.0];
    let layout = descriptor_layout::ScenePipelineDescriptorLayout {
        sampled_slots: Vec::new(),
        input_attachment_slots: Vec::new(),
        material_uniform_enabled: false,
        skinning_storage_enabled: false,
        particle_storage_enabled: false,
        scene_owned_uniform_count: 0,
    };

    let (_, commands) =
        scene_descriptor_plan_inputs(&storage, &[draw], &[], &layout, &[7], &[None]);

    assert_eq!(
        commands[0].primitive,
        SceneRenderingDeviceDrawPrimitive::ObjectMesh
    );
    assert_eq!(commands[0].vertex_buffer_byte_offset, Some(37 * 52));
    assert_eq!(commands[0].vertex_offset, 0);
    assert_eq!(commands[0].first_index, 19);
    assert_eq!(commands[0].index_count, 6);
}

fn object_mesh_draw() -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        particle_index: crate::engine::scene::INVALID_PARTICLE_INDEX,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
        uv_inset_texels: 0.0,
        skinning_palette_start: 2,
        skinning_palette_count: 3,
        resolved_color: crate::engine::scene::SceneVec3::ONE,
        resolved_alpha: 1.0,
        apply_resolved_visual: true,
        effect_batch_atlas_tile: u32::MAX,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        instance_count: 1,
    }
}

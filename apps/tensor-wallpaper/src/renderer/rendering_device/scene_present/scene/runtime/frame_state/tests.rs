use super::*;

#[test]
fn child_particle_spawn_policy_matches_compute_pre_roll_formula() {
    let mut particle = crate::engine::scene::SceneParticleSystemRecord::unsupported(
        crate::engine::scene::SceneObjectHandle(0),
        crate::engine::scene::SceneResourceId(0),
        crate::engine::scene::SceneMaterialHandle(0),
        0,
        3,
        1.0,
        0.0,
        1.0,
    );
    particle.rate = 1.2;
    assert_eq!(particle_spawned_count(&particle, 0.8), 0);
    assert_eq!(particle_spawned_count(&particle, 0.84), 1);
    particle.start_time = 60.0;
    assert_eq!(particle_spawned_count(&particle, 0.0), 72);
}
use crate::engine::scene::semantic_world::{
    ResolvedObjectEffectState, ResolvedObjectState, ResolvedPuppetBoneMatrix,
    ResolvedPuppetBonePalette, SemanticEntity,
};
use crate::engine::scene::{
    SceneEffectHandle, SceneMaterialHandle, SceneRenderGraphActivationPolicy, SceneRenderPassKind,
    SceneRenderTargetKind, SceneRenderingDevicePassNode, SceneRenderingDevicePuppetBoneMatrix,
    SceneRenderingDevicePuppetBonePalette, SceneStringId,
};
use crate::renderer::rendering_device::RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;

#[test]
fn dynamic_frame_update_retains_authored_texture_projection_domain() {
    let object = SceneObjectHandle(0);
    let storage = SceneStorage::from_document(crate::engine::scene::SceneBinaryDocument {
        objects: vec![crate::engine::scene::SceneObjectRecord {
            id: object,
            we_id: 7,
            name: SceneStringId::NONE,
            kind: crate::engine::scene::SceneObjectKind::Image,
            resource: crate::engine::scene::SceneResourceId::NONE,
            material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
            parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: crate::engine::scene::SceneVec3::default(),
            angles: crate::engine::scene::SceneVec3::default(),
            scale: crate::engine::scene::SceneVec3::ONE,
            camera_zoom: 1.0,
            color: crate::engine::scene::SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }],
        ..crate::engine::scene::SceneBinaryDocument::default()
    })
    .expect("frame projection storage");
    let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 0,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    graph.puppet_bone_palettes.clear();
    graph.puppet_bone_matrices.clear();
    let mut draw = effect_draw(
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        0,
        u32::MAX,
    );
    draw.primitive = SceneRenderingDeviceDrawPrimitive::ObjectMesh;
    draw.object = object;
    draw.projection_domain =
        crate::engine::scene::SceneRenderingDeviceProjectionDomain::AuthoredTexture {
            width: 415,
            height: 405,
        };
    draw.effect_binding_count = 0;
    graph.mesh_draws = vec![draw];
    let mut topology = SceneFrameTopology::from_owned_graph(graph);
    let world_matrix = [
        0.5, 0.1, 0.0, 0.0, -0.2, 0.75, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1_234.0, -456.0, 20.0, 1.0,
    ];
    let frame = ResolvedSemanticFrame::from_resolved_parts(
        vec![ResolvedObjectState {
            entity: SemanticEntity::from_raw(0),
            object,
            object_index: 0,
            parent: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
            parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            local_matrix: world_matrix,
            world_matrix,
            render_world_matrix: world_matrix,
            camera_zoom: 1.0,
            self_visible: true,
            resolved_visible: true,
            self_color: crate::engine::scene::SceneVec3::ONE,
            resolved_color: crate::engine::scene::SceneVec3::ONE,
            self_alpha: 1.0,
            resolved_alpha: 1.0,
            sort_order: 0,
            mesh_binding_start: 0,
            mesh_binding_count: 1,
            puppet_index: u32::MAX,
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    let updated = topology
        .update_dynamic_graph(&storage, &frame, 3.0)
        .expect("dynamic authored-texture graph");
    let updated = &updated.mesh_draws[0];
    let expected = [
        [2.0 / 415.0, 0.0, 0.0, 0.0],
        [0.0, -2.0 / 405.0, 0.0, 0.0],
        [0.0, 0.0, 0.0005, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    assert_eq!(updated.clip_transform, expected);
    assert_eq!(updated.effect_model_view_projection_matrix, expected);
    assert_eq!(updated.render_world_matrix[0][3], 1_234.0);
}

#[test]
fn hidden_passthrough_effect_switches_pipeline_without_affecting_material_stage_draw() {
    let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 0,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    graph.mesh_draws = vec![
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
            0,
            3,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages,
            0,
            4,
        ),
    ];
    let mut commands = vec![draw_command(10, Some(20)), draw_command(11, None)];

    update_effect_draw_pipelines(&graph, &mut commands).expect("typed visibility pipelines");

    assert_eq!(commands[0].pipeline_index, 20);
    assert_eq!(commands[1].pipeline_index, 11);

    graph.mesh_draws[0].resolved_effect_visibility_mask = 1;
    update_effect_draw_pipelines(&graph, &mut commands).expect("visible authored pipeline");
    assert_eq!(commands[0].pipeline_index, 10);
}
#[test]
fn effect_only_framebuffer_graph_disables_every_draw_when_all_effects_are_hidden() {
    let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 0,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    let pass = |pass_id,
                role,
                effect_binding_start,
                effect_binding_count,
                effect_visibility_policy,
                mesh_draw_start| SceneRenderingDevicePassNode {
        graph_index: 4,
        graph_activation_policy: SceneRenderGraphActivationPolicy::AnyEffectVisible,
        pass_record_index: pass_id,
        pass_id,
        role,
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start,
        effect_binding_count,
        effect_visibility_policy,
        mesh_draw_start,
        mesh_draw_count: 1,
    };
    graph.pass_nodes = vec![
        pass(
            0,
            SceneRenderPassKind::BaseMaterial,
            u32::MAX,
            0,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            0,
        ),
        pass(
            1,
            SceneRenderPassKind::EffectMaterial,
            0,
            1,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
            1,
        ),
        pass(
            2,
            SceneRenderPassKind::SceneComposite,
            u32::MAX,
            0,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            2,
        ),
    ];
    graph.mesh_draws = vec![
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            0,
            u32::MAX,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
            0,
            0,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            0,
            u32::MAX,
        ),
    ];
    let mut commands = vec![
        draw_command(10, None),
        draw_command(11, Some(21)),
        draw_command(12, None),
    ];
    let mut frame = frame_with_effect_visibility(false);
    let sampled_target_producers = sampled_target_producer_topology(&graph);

    update_draw_visibility(&graph, &sampled_target_producers, &frame, &mut commands);
    assert!(commands.iter().all(|command| !command.enabled));

    frame.object_effects[0].resolved_visible = true;
    update_draw_visibility(&graph, &sampled_target_producers, &frame, &mut commands);
    assert!(commands.iter().all(|command| command.enabled));
}

#[test]
fn runtime_effect_branch_selects_direct_base_only_when_the_effect_is_hidden() {
    let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 0,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    let pass = |pass_id, policy, mesh_draw_start| SceneRenderingDevicePassNode {
        graph_index: 5,
        graph_activation_policy: SceneRenderGraphActivationPolicy::Always,
        pass_record_index: pass_id,
        pass_id,
        role: if pass_id == 3 {
            SceneRenderPassKind::BaseMaterial
        } else {
            SceneRenderPassKind::EffectMaterial
        },
        target: if pass_id == 3 {
            SceneRenderTargetKind::SceneColor
        } else {
            SceneRenderTargetKind::ImageLocalMain
        },
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: 0,
        effect_binding_count: 1,
        effect_visibility_policy: policy,
        mesh_draw_start,
        mesh_draw_count: 1,
    };
    graph.pass_nodes = vec![
        pass(
            0,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            0,
        ),
        pass(
            1,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            1,
        ),
        pass(
            2,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            2,
        ),
        pass(
            3,
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::NoneVisible,
            3,
        ),
    ];
    graph.mesh_draws = vec![
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            0,
            0,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            0,
            0,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible,
            0,
            0,
        ),
        effect_draw(
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::NoneVisible,
            0,
            0,
        ),
    ];
    let sampled_target_producers = sampled_target_producer_topology(&graph);
    let mut commands = vec![
        draw_command(10, None),
        draw_command(11, None),
        draw_command(12, None),
        draw_command(13, None),
    ];
    let mut frame = frame_with_effect_visibility(false);

    update_draw_visibility(&graph, &sampled_target_producers, &frame, &mut commands);
    assert_eq!(
        commands
            .iter()
            .map(|command| command.enabled)
            .collect::<Vec<_>>(),
        [false, false, false, true]
    );

    frame.object_effects[0].resolved_visible = true;
    update_draw_visibility(&graph, &sampled_target_producers, &frame, &mut commands);
    assert_eq!(
        commands
            .iter()
            .map(|command| command.enabled)
            .collect::<Vec<_>>(),
        [true, true, true, false]
    );
}

#[test]
fn skinning_payload_prefixes_identity_and_packs_alpha_in_std430_entry() {
    let graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 41,
        parent_index: -1,
        matrix: [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ],
        alpha: 0.375,
    });

    let payload = pack_scene_skinning_palette(&graph);

    assert_eq!(
        payload.len(),
        2 * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
    );
    assert_eq!(payload_f32(&payload, 0), 1.0);
    assert_eq!(payload_f32(&payload, 60), 1.0);
    assert_eq!(payload_f32(&payload, 64), 1.0);
    assert_eq!(
        payload_f32(
            &payload,
            RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
        ),
        1.0
    );
    assert_eq!(
        payload_f32(
            &payload,
            RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 60
        ),
        16.0
    );
    assert_eq!(
        payload_f32(
            &payload,
            RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 64
        ),
        0.375
    );
}

#[test]
fn topology_ignores_dynamic_matrix_and_alpha_values() {
    let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 41,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    let mut frame = setup.clone();
    frame.puppet_bone_matrices[0].matrix = [[2.0; 4]; 4];
    frame.puppet_bone_matrices[0].alpha = 0.25;

    SceneFrameTopology::from_graph(&setup)
        .validate(&frame, 1.0)
        .expect("dynamic bone values preserve topology");
}

#[test]
fn topology_rejects_dynamic_bone_reordering() {
    let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 41,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    let mut frame = setup.clone();
    frame.puppet_bone_matrices[0].bone_index = 42;

    let error = SceneFrameTopology::from_graph(&setup)
        .validate(&frame, 1.0)
        .unwrap_err();
    assert!(error.contains("puppet bone topology changed"));
    assert!(error.contains("index 0"));
}

#[test]
fn retained_graph_updates_dynamic_palette_matrix_and_alpha_in_place() {
    let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
        puppet_index: 0,
        bone_index: 41,
        parent_index: -1,
        matrix: [[0.0; 4]; 4],
        alpha: 1.0,
    });
    let matrix = [
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
    ];
    let frame = ResolvedSemanticFrame {
        objects: Vec::new(),
        object_effects: Vec::new(),
        attachment_links: Vec::new(),
        puppet_bone_palettes: vec![ResolvedPuppetBonePalette {
            object: SceneObjectHandle(0),
            puppet_index: 0,
            bone_start: 0,
            bone_count: 1,
            resolved_visible: true,
        }],
        puppet_bone_matrices: vec![ResolvedPuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix,
            alpha: 0.25,
        }],
        audio_band_material_values: Vec::new(),
        material_scalar_values: Vec::new(),
        script_text_values: Vec::new(),
        media_clock: None,
        video_frame: None,
        parallax_position: [0.5; 2],
        particle_camera_parallax_translation: [0.0; 2],
        visible_object_count: 0,
        visible_mesh_binding_count: 0,
        visible_effect_instance_count: 0,
        visible_effect_pass_count: 0,
        visible_effect_fbo_count: 0,
        visible_puppet_binding_count: 0,
        visible_puppet_bone_matrix_count: 1,
    };

    update_puppet_palettes(&mut graph, &frame, 2.0).expect("stable palette topology");

    assert_eq!(
        graph.puppet_bone_matrices[0].matrix,
        [
            [1.0, 5.0, 9.0, 13.0],
            [2.0, 6.0, 10.0, 14.0],
            [3.0, 7.0, 11.0, 15.0],
            [4.0, 8.0, 12.0, 16.0],
        ]
    );
    assert_eq!(graph.puppet_bone_matrices[0].alpha, 0.25);
}

fn graph_with_bone(bone: SceneRenderingDevicePuppetBoneMatrix) -> SceneRenderingDeviceGraphPlan {
    SceneRenderingDeviceGraphPlan {
        pass_nodes: Vec::new(),
        target_allocations: Vec::new(),
        effect_batches: Vec::new(),
        effect_batch_instances: Vec::new(),
        sampled_bindings: Vec::new(),
        material_sampled_bindings: Vec::new(),
        mesh_draws: Vec::new(),
        puppet_bone_palettes: vec![SceneRenderingDevicePuppetBonePalette {
            object: SceneObjectHandle(0),
            puppet_index: 0,
            bone_matrix_start: 0,
            bone_matrix_count: 1,
            resolved_visible: true,
        }],
        puppet_bone_matrices: vec![bone],
        particle_gpu_emitters: Vec::new(),
        resolved_object_count: 1,
        resolved_visible_object_count: 1,
        resolved_attachment_link_count: 0,
        resolved_visible_effect_instance_count: 0,
        resolved_visible_effect_pass_count: 0,
        resolved_visible_effect_fbo_count: 0,
        descriptor_heap_required: true,
        descriptor_heap_resource_count: 1,
        descriptor_heap_sampled_image_count: 0,
        descriptor_heap_uniform_buffer_count: 0,
        descriptor_heap_storage_buffer_count: 1,
        descriptor_heap_sampler_count: 0,
        graph_physical_target_count: 0,
        graph_aliased_target_count: 0,
        fifo_latest_ready_present_required: true,
    }
}

fn frame_with_effect_visibility(resolved_visible: bool) -> ResolvedSemanticFrame {
    ResolvedSemanticFrame {
        objects: Vec::new(),
        object_effects: vec![ResolvedObjectEffectState {
            binding_index: 0,
            entity: SemanticEntity::from_raw(0),
            object: SceneObjectHandle(0),
            object_index: 0,
            effect: SceneEffectHandle(0),
            effect_index: 0,
            instance_id: 0,
            self_visible: resolved_visible,
            object_resolved_visible: true,
            resolved_visible,
            pass_start: 0,
            pass_count: 1,
            fbo_start: 0,
            fbo_count: 0,
        }],
        attachment_links: Vec::new(),
        puppet_bone_palettes: Vec::new(),
        puppet_bone_matrices: Vec::new(),
        audio_band_material_values: Vec::new(),
        material_scalar_values: Vec::new(),
        script_text_values: Vec::new(),
        media_clock: None,
        video_frame: None,
        parallax_position: [0.5; 2],
        particle_camera_parallax_translation: [0.0; 2],
        visible_object_count: 0,
        visible_mesh_binding_count: 0,
        visible_effect_instance_count: usize::from(resolved_visible),
        visible_effect_pass_count: usize::from(resolved_visible),
        visible_effect_fbo_count: 0,
        visible_puppet_binding_count: 0,
        visible_puppet_bone_matrix_count: 0,
    }
}

fn effect_draw(
    policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy,
    visibility_mask: u32,
    binding_start: u32,
) -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        particle_index: crate::engine::scene::INVALID_PARTICLE_INDEX,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
        resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        effect_texture_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [1.0; 2],
        uv_inset_texels: 0.0,
        skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: crate::engine::scene::SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        resolved_alpha: 1.0,
        apply_resolved_visual: false,
        effect_batch_atlas_tile: crate::engine::scene::INVALID_OBJECT_ID,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: binding_start,
        effect_binding_count: 1,
        effect_visibility_policy: policy,
        resolved_effect_visibility_mask: visibility_mask,
        object: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
        material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
        vertex_start: 0,
        vertex_count: 3,
        index_start: 0,
        index_count: 3,
        instance_count: 1,
    }
}

fn draw_command(
    authored_pipeline_index: u32,
    disabled_pipeline_index: Option<u32>,
) -> SceneGpuDrawCommand {
    SceneGpuDrawCommand {
        enabled: true,
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        pipeline_index: authored_pipeline_index,
        authored_pipeline_index,
        disabled_pipeline_index,
        first_index: 0,
        index_count: 3,
        vertex_offset: 0,
        vertex_buffer_byte_offset: None,
        vertex_count: 3,
        instance_count: 1,
        instance_capacity: 1,
        first_instance: 0,
        dynamic_text: false,
        video_media_instance: None,
        video_vertex_byte_offset: None,
        particle_indirect_index: None,
        resource_descriptor_base: 0,
        material_resource_descriptor: None,
        skinning_resource_descriptor: None,
        particle_resource_descriptor: None,
        scene_owned_uniform_descriptor_base: 0,
        sampled_resource_descriptor_base: 0,
        input_attachment_resource_descriptor_base: 0,
        sampler_descriptor_base: 0,
        descriptor_push: None,
        disabled_descriptor_push: None,
        skinning_byte_offset: 0,
        skinning_byte_count: 0,
        scissor: None,
    }
}

fn payload_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}

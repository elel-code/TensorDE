use super::{SceneGpuDrawCommand, sampled_target_producer_topology, update_draw_visibility};

use crate::engine::scene::semantic_world::{
    ResolvedObjectState, ResolvedSemanticFrame, SemanticEntity,
};
use crate::engine::scene::{
    SceneMaterialHandle, SceneObjectHandle, SceneRenderBindingKind,
    SceneRenderEffectVisibilityPolicy, SceneRenderGraphActivationPolicy, SceneRenderPassKind,
    SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
    SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation, SceneStringId,
};

#[test]
fn external_consumer_keeps_hidden_offscreen_producer_chain_live() {
    let pass = |graph_index, pass_id, role, target, target_name, mesh_draw_start| {
        SceneRenderingDevicePassNode {
            graph_index,
            graph_activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_record_index: pass_id,
            pass_id,
            role,
            target,
            target_name,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start,
            mesh_draw_count: 1,
        }
    };
    let mut draw = effect_draw();
    draw.object = SceneObjectHandle(0);
    draw.resolved_object_index = 0;
    let external_target_name = SceneStringId(17);
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![
            pass(
                2,
                0,
                SceneRenderPassKind::BaseMaterial,
                SceneRenderTargetKind::ImageLocalMain,
                SceneStringId::NONE,
                0,
            ),
            pass(
                2,
                1,
                SceneRenderPassKind::EffectMaterial,
                SceneRenderTargetKind::FirstClassEffectTarget,
                external_target_name,
                1,
            ),
            pass(
                2,
                2,
                SceneRenderPassKind::SceneComposite,
                SceneRenderTargetKind::SceneColor,
                SceneStringId::NONE,
                2,
            ),
            pass(
                3,
                0,
                SceneRenderPassKind::SceneComposite,
                SceneRenderTargetKind::SceneColor,
                SceneStringId::NONE,
                3,
            ),
            pass(
                4,
                0,
                SceneRenderPassKind::BaseMaterial,
                SceneRenderTargetKind::ImageLocalMain,
                SceneStringId::NONE,
                4,
            ),
        ],
        target_allocations: vec![
            SceneRenderingDeviceTargetAllocation {
                graph_index: 2,
                target: SceneRenderTargetKind::ImageLocalMain,
                target_name: SceneStringId::NONE,
                first_write_pass_id: 0,
                last_use_pass_id: 1,
                physical_slot: 0,
                width: 320,
                height: 180,
            },
            SceneRenderingDeviceTargetAllocation {
                graph_index: 2,
                target: SceneRenderTargetKind::FirstClassEffectTarget,
                target_name: external_target_name,
                first_write_pass_id: 1,
                last_use_pass_id: 2,
                physical_slot: 1,
                width: 320,
                height: 180,
            },
            SceneRenderingDeviceTargetAllocation {
                graph_index: 4,
                target: SceneRenderTargetKind::ImageLocalMain,
                target_name: SceneStringId::NONE,
                first_write_pass_id: 0,
                last_use_pass_id: 0,
                physical_slot: 2,
                width: 64,
                height: 64,
            },
        ],
        effect_batches: Vec::new(),
        effect_batch_instances: Vec::new(),
        sampled_bindings: vec![
            sampled_binding(
                1,
                2,
                1,
                SceneRenderBindingKind::PreviousGraphTarget,
                SceneRenderTargetKind::ImageLocalMain,
                SceneStringId::NONE,
            ),
            sampled_binding(
                2,
                2,
                2,
                SceneRenderBindingKind::PreviousGraphTarget,
                SceneRenderTargetKind::FirstClassEffectTarget,
                external_target_name,
            ),
            sampled_binding(
                3,
                2,
                3,
                SceneRenderBindingKind::EffectTarget,
                SceneRenderTargetKind::FirstClassEffectTarget,
                external_target_name,
            ),
        ],
        material_sampled_bindings: Vec::new(),
        mesh_draws: vec![draw, draw, draw, effect_draw(), draw],
        puppet_bone_palettes: Vec::new(),
        puppet_bone_matrices: Vec::new(),
        particle_gpu_emitters: Vec::new(),
        resolved_object_count: 1,
        resolved_visible_object_count: 0,
        resolved_attachment_link_count: 0,
        resolved_visible_effect_instance_count: 0,
        resolved_visible_effect_pass_count: 0,
        resolved_visible_effect_fbo_count: 0,
        descriptor_heap_required: true,
        descriptor_heap_resource_count: 0,
        descriptor_heap_sampled_image_count: 0,
        descriptor_heap_uniform_buffer_count: 0,
        descriptor_heap_storage_buffer_count: 0,
        descriptor_heap_sampler_count: 0,
        graph_physical_target_count: 0,
        graph_aliased_target_count: 0,
        fifo_latest_ready_present_required: true,
    };
    let mut commands = vec![
        draw_command(10),
        draw_command(11),
        draw_command(12),
        draw_command(13),
        draw_command(14),
    ];
    let sampled_target_producers = sampled_target_producer_topology(&graph);

    update_draw_visibility(
        &graph,
        &sampled_target_producers,
        &hidden_object_frame(),
        &mut commands,
    );

    assert!(commands[0].enabled, "the recursive local source stays live");
    assert!(
        commands[1].enabled,
        "the externally sampled producer stays live"
    );
    assert!(
        !commands[2].enabled,
        "the hidden object's terminal scene composite stays disabled"
    );
    assert!(commands[3].enabled, "the external consumer stays enabled");
    assert!(
        !commands[4].enabled,
        "an unconsumed hidden offscreen pass stays disabled"
    );
    assert_eq!(
        graph.pass_nodes.len(),
        5,
        "retained pass topology stays intact"
    );
}

fn sampled_binding(
    pass_node_index: u32,
    producer_graph_index: u32,
    mesh_draw_start: u32,
    kind: SceneRenderBindingKind,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> SceneRenderingDeviceSampledBinding {
    SceneRenderingDeviceSampledBinding {
        pass_node_index,
        graph_index: producer_graph_index,
        mesh_draw_start,
        mesh_draw_count: 1,
        kind,
        slot: 0,
        target,
        target_name,
        access: SceneRenderingDeviceImageAccess::SampledImage,
    }
}

fn hidden_object_frame() -> ResolvedSemanticFrame {
    ResolvedSemanticFrame::from_resolved_parts(
        vec![ResolvedObjectState {
            entity: SemanticEntity::from_raw(0),
            object: SceneObjectHandle(0),
            object_index: 0,
            parent: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
            parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            local_matrix: [0.0; 16],
            world_matrix: [0.0; 16],
            render_world_matrix: [0.0; 16],
            self_visible: false,
            resolved_visible: false,
            self_color: crate::engine::scene::SceneVec3::ONE,
            resolved_color: crate::engine::scene::SceneVec3::ONE,
            self_alpha: 1.0,
            resolved_alpha: 1.0,
            sort_order: 0,
            mesh_binding_start: 0,
            mesh_binding_count: 1,
            puppet_index:
                crate::engine::scene::semantic_world::resolved_frame::INVALID_RESOLVED_INDEX,
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn effect_draw() -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        shader_key: SceneStringId::NONE,
        mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
        resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [1.0; 2],
        skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: crate::engine::scene::SceneVec3::ONE,
        resolved_alpha: 1.0,
        apply_resolved_visual: false,
        effect_batch_atlas_tile: crate::engine::scene::INVALID_OBJECT_ID,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: u32::MAX,
        effect_binding_count: 1,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
        material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
        vertex_start: 0,
        vertex_count: 3,
        index_start: 0,
        index_count: 3,
        instance_count: 1,
    }
}

fn draw_command(pipeline_index: u32) -> SceneGpuDrawCommand {
    SceneGpuDrawCommand {
        enabled: true,
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        pipeline_index,
        authored_pipeline_index: pipeline_index,
        disabled_pipeline_index: None,
        first_index: 0,
        index_count: 3,
        vertex_offset: 0,
        vertex_count: 3,
        instance_count: 1,
        instance_capacity: 1,
        first_instance: 0,
        dynamic_text: false,
        particle_indirect_index: None,
        resource_descriptor_base: 0,
        material_resource_descriptor: None,
        skinning_resource_descriptor: None,
        scene_owned_uniform_descriptor_base: 0,
        sampled_resource_descriptor_base: 0,
        input_attachment_resource_descriptor_base: 0,
        sampler_descriptor_base: 0,
        native_descriptor_push: None,
        disabled_native_descriptor_push: None,
        skinning_byte_offset: 0,
        skinning_byte_count: 0,
        scissor: None,
    }
}

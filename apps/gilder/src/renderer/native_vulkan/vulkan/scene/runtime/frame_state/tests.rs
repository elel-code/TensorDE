    use super::*;
    use crate::engine::scene::semantic_world::{
        ResolvedObjectEffectState, ResolvedPuppetBoneMatrix, ResolvedPuppetBonePalette,
        SemanticEntity,
    };
    use crate::engine::scene::{
        SceneEffectHandle, SceneMaterialHandle, SceneRenderGraphActivationPolicy,
        SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDevicePassNode,
        SceneRenderingDevicePuppetBoneMatrix, SceneRenderingDevicePuppetBonePalette, SceneStringId,
    };
    use crate::renderer::native_vulkan::NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;

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

        update_draw_visibility(&graph, &frame, &mut commands);
        assert!(commands.iter().all(|command| !command.enabled));

        frame.object_effects[0].resolved_visible = true;
        update_draw_visibility(&graph, &frame, &mut commands);
        assert!(commands.iter().all(|command| command.enabled));
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
            2 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
        );
        assert_eq!(payload_f32(&payload, 0), 1.0);
        assert_eq!(payload_f32(&payload, 60), 1.0);
        assert_eq!(payload_f32(&payload, 64), 1.0);
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
            ),
            1.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 60
            ),
            16.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 64
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

    fn graph_with_bone(
        bone: SceneRenderingDevicePuppetBoneMatrix,
    ) -> SceneRenderingDeviceGraphPlan {
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
            shader_key: crate::engine::scene::SceneStringId::NONE,
            mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
            resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
            render_world_matrix: [[0.0; 4]; 4],
            clip_transform: [[0.0; 4]; 4],
            effect_model_view_projection_matrix: [[0.0; 4]; 4],
            authored_source_extent: [1.0; 2],
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
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: None,
        }
    }

    fn payload_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }

use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneObjectHandle, SceneRenderEffectVisibilityPolicy,
    SceneRenderGraphActivationPolicy, SceneRenderPassKind, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode,
    SceneRenderingDeviceTargetAllocation, SceneStorage, SceneStringId, SceneTargetExtentDomain,
};

#[test]
fn alignment_uses_the_device_limit_without_power_of_two_assumptions() {
    assert_eq!(align_up(0, 192, "test").unwrap(), 0);
    assert_eq!(align_up(176, 192, "test").unwrap(), 192);
    assert_eq!(align_up(240, 192, "test").unwrap(), 384);
}

#[test]
fn diagnostic_payload_slices_retain_draw_and_lane_order() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 16,
        slices: vec![
            SceneOwnedUniformSlicePlan {
                draw_index: 3,
                descriptor_lane: 0,
                byte_offset: 0,
                byte_size: 4,
                members: Vec::new(),
            },
            SceneOwnedUniformSlicePlan {
                draw_index: 4,
                descriptor_lane: 0,
                byte_offset: 4,
                byte_size: 4,
                members: Vec::new(),
            },
            SceneOwnedUniformSlicePlan {
                draw_index: 3,
                descriptor_lane: 1,
                byte_offset: 8,
                byte_size: 8,
                members: Vec::new(),
            },
        ],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0; 2],
    };
    let payload = (0u8..16).collect::<Vec<_>>();

    let slices = plan
        .payload_slices_for_draw(3, &payload)
        .expect("diagnostic slices");

    assert_eq!(slices, [&payload[0..4], &payload[8..16]]);
    assert!(plan.payload_slices_for_draw(3, &payload[..12]).is_err());
}

#[test]
fn retained_payload_updates_matrices_resolution_and_scalar_without_allocating_sources() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 320,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 220,
            members: vec![
                member(0, 64, SceneOwnedRetainedSource::ModelViewProjectionMatrix),
                member(
                    64,
                    64,
                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix,
                ),
                member(128, 64, SceneOwnedRetainedSource::LayerModelMatrix),
                member(
                    192,
                    16,
                    SceneOwnedRetainedSource::SampledTextureResolution {
                        sampled_slot_index: 0,
                    },
                ),
                member(
                    208,
                    4,
                    SceneOwnedRetainedSource::MaterialConstant {
                        constant_index: 9,
                        default_values: vec![0.25],
                    },
                ),
                member(212, 4, SceneOwnedRetainedSource::SceneTime),
                member(216, 4, SceneOwnedRetainedSource::FrameDelta),
            ],
        }],
        sampled_slots: vec![0],
        phase_resolutions: vec![vec![[128.0, 64.0, 100.0, 50.0]]],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let mut draw = draw();
    draw.clip_transform[0][0] = 2.0;
    draw.effect_model_view_projection_matrix[0][0] = 9.0;
    draw.render_world_matrix[3][0] = 17.0;
    let override_value = ResolvedMaterialScalarValue {
        object: draw.object,
        constant_index: 9,
        value: 0.75,
    };
    let mut payload = vec![0xcc; 320];

    plan.write_payload(
        &[draw],
        SceneOwnedUniformFrameInputs {
            scalar_overrides: &[override_value],
            scene_time_seconds: 1.25,
            frame_delta_seconds: 0.5,
            audio_spectrum: &StereoSpectrum64::ZERO,
            parallax_position: [0.25, 0.75],
            sampled_binding_phase: 0,
        },
        &mut payload,
    )
    .expect("retained payload");

    assert_eq!(read_f32(&payload, 0), 2.0);
    assert_eq!(read_f32(&payload, 64), 9.0);
    assert_eq!(read_f32(&payload, 176), 17.0);
    assert_eq!(read_f32(&payload, 192), 128.0);
    assert_eq!(read_f32(&payload, 204), 50.0);
    assert_eq!(read_f32(&payload, 208), 0.75);
    assert_eq!(read_f32(&payload, 212), 1.25);
    assert_eq!(read_f32(&payload, 216), 0.5);
    assert!(payload[220..].iter().all(|byte| *byte == 0xcc));
}

#[test]
fn retained_payload_applies_object_visual_globals_only_at_the_selected_draw_boundary() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 20,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 20,
            members: vec![
                member(0, 16, SceneOwnedRetainedSource::ObjectColor4),
                member(16, 4, SceneOwnedRetainedSource::ObjectAlpha),
            ],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let mut draw = draw();
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.2,
        y: 0.4,
        z: 0.6,
    };
    draw.resolved_alpha = 0.75;
    let mut payload = vec![0; 20];

    draw.apply_resolved_visual = true;
    plan.write_payload(&[draw], SceneOwnedUniformFrameInputs::INITIAL, &mut payload)
        .expect("object visual payload");
    assert_eq!(read_f32(&payload, 0), 0.2);
    assert_eq!(read_f32(&payload, 4), 0.4);
    assert_eq!(read_f32(&payload, 8), 0.6);
    assert_eq!(read_f32(&payload, 12), 0.75);
    assert_eq!(read_f32(&payload, 16), 0.75);

    draw.apply_resolved_visual = false;
    plan.write_payload(&[draw], SceneOwnedUniformFrameInputs::INITIAL, &mut payload)
        .expect("neutral object visual payload");
    for offset in [0, 4, 8, 12, 16] {
        assert_eq!(read_f32(&payload, offset), 1.0);
    }
}

#[test]
fn retained_payload_writes_the_frame_parallax_position() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 8,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 8,
            members: vec![member(0, 8, SceneOwnedRetainedSource::ParallaxPosition)],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let mut payload = vec![0; 8];

    plan.write_payload(
        &[draw()],
        SceneOwnedUniformFrameInputs {
            parallax_position: [0.25, 0.75],
            ..SceneOwnedUniformFrameInputs::INITIAL
        },
        &mut payload,
    )
    .expect("parallax position payload");

    assert_eq!(read_f32(&payload, 0), 0.25);
    assert_eq!(read_f32(&payload, 4), 0.75);
}

#[test]
fn retained_payload_writes_the_current_render_target_reciprocal_extent() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 8,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 8,
            members: vec![member(
                0,
                8,
                SceneOwnedRetainedSource::CurrentRenderTargetTexelSize {
                    texel_size: [1.0 / 960.0, 1.0 / 540.0],
                },
            )],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let mut payload = vec![0; 8];

    plan.write_payload(
        &[draw()],
        SceneOwnedUniformFrameInputs::INITIAL,
        &mut payload,
    )
    .expect("current render-target texel size payload");

    assert_eq!(read_f32(&payload, 0), 1.0 / 960.0);
    assert_eq!(read_f32(&payload, 4), 1.0 / 540.0);
}

#[test]
fn current_render_target_texel_size_uses_each_pass_logical_target_extent() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let mut second_draw = draw();
    second_draw.object = SceneObjectHandle(4);
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![
            pass_node(0, SceneRenderTargetKind::SceneColor, SceneStringId::NONE),
            pass_node(1, SceneRenderTargetKind::NamedFbo, SceneStringId(17)),
        ],
        target_allocations: vec![SceneRenderingDeviceTargetAllocation {
            graph_index: 1,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(17),
            first_write_pass_id: 1,
            last_use_pass_id: 1,
            physical_slot: 0,
            width: 960,
            height: 540,
            extent_domain: SceneTargetExtentDomain::OwnerAuthored,
        }],
        effect_batches: Vec::new(),
        effect_batch_instances: Vec::new(),
        sampled_bindings: Vec::new(),
        material_sampled_bindings: Vec::new(),
        mesh_draws: vec![draw(), second_draw],
        puppet_bone_palettes: Vec::new(),
        puppet_bone_matrices: Vec::new(),
        particle_gpu_emitters: Vec::new(),
        resolved_object_count: 2,
        resolved_visible_object_count: 2,
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
        graph_physical_target_count: 1,
        graph_aliased_target_count: 0,
        fifo_latest_ready_present_required: true,
    };

    assert_eq!(
        current_render_target_texel_size(&storage, &graph, 0, [3840, 2160]).unwrap(),
        [1.0 / 3840.0, 1.0 / 2160.0]
    );
    assert_eq!(
        current_render_target_texel_size(&storage, &graph, 1, [3840, 2160]).unwrap(),
        [1.0 / 960.0, 1.0 / 540.0]
    );
}

#[test]
fn retained_payload_writes_the_inverse_effect_texture_projection() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 64,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 64,
            members: vec![member(
                0,
                64,
                SceneOwnedRetainedSource::EffectTextureProjectionMatrixInverse,
            )],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let mut draw = draw();
    draw.effect_texture_projection_matrix = [
        [2.0, 0.0, 0.0, 8.0],
        [0.0, -4.0, 0.0, 12.0],
        [0.0, 0.0, 1.0, 3.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut payload = vec![0; 64];

    plan.write_payload(&[draw], SceneOwnedUniformFrameInputs::INITIAL, &mut payload)
        .expect("inverse effect texture projection payload");

    assert_eq!(read_f32(&payload, 0), 0.5);
    assert_eq!(read_f32(&payload, 12), -4.0);
    assert_eq!(read_f32(&payload, 20), -0.25);
    assert_eq!(read_f32(&payload, 28), 3.0);
    assert_eq!(read_f32(&payload, 40), 1.0);
    assert_eq!(read_f32(&payload, 44), -3.0);
    assert_eq!(read_f32(&payload, 60), 1.0);
}

#[test]
fn retained_payload_applies_cover_only_to_projection_matrices() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 192,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 192,
            members: vec![
                member(0, 64, SceneOwnedRetainedSource::ModelViewProjectionMatrix),
                member(
                    64,
                    64,
                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix,
                ),
                member(128, 64, SceneOwnedRetainedSource::LayerModelMatrix),
            ],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.34375, 1.0],
    };
    let mut draw = draw();
    draw.clip_transform[0] = [2.0, 3.0, 4.0, 5.0];
    draw.clip_transform[1] = [6.0, 7.0, 8.0, 9.0];
    draw.effect_model_view_projection_matrix[0] = [10.0, 11.0, 12.0, 13.0];
    draw.effect_model_view_projection_matrix[1] = [14.0, 15.0, 16.0, 17.0];
    draw.render_world_matrix[0] = [18.0, 19.0, 20.0, 21.0];
    let original = draw;
    let mut payload = vec![0; 192];

    plan.write_payload(&[draw], SceneOwnedUniformFrameInputs::INITIAL, &mut payload)
        .expect("covered retained payload");

    assert_eq!(read_f32(&payload, 0), 2.0 * 1.34375);
    assert_eq!(read_f32(&payload, 12), 5.0 * 1.34375);
    assert_eq!(read_f32(&payload, 16), 6.0);
    assert_eq!(read_f32(&payload, 64), 10.0 * 1.34375);
    assert_eq!(read_f32(&payload, 76), 13.0 * 1.34375);
    assert_eq!(read_f32(&payload, 80), 14.0);
    assert_eq!(read_f32(&payload, 128), 18.0);
    assert_eq!(read_f32(&payload, 140), 21.0);
    assert_eq!(draw, original);
}

#[test]
fn retained_payload_does_not_apply_scene_cover_to_authored_texture_projection() {
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 128,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 128,
            members: vec![
                member(0, 64, SceneOwnedRetainedSource::ModelViewProjectionMatrix),
                member(
                    64,
                    64,
                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix,
                ),
            ],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.013_831_4, 1.0],
    };
    let mut draw = draw();
    draw.projection_domain =
        crate::engine::scene::SceneRenderingDeviceProjectionDomain::AuthoredTexture {
            width: 415,
            height: 405,
        };
    draw.clip_transform = [
        [2.0 / 415.0, 0.0, 0.0, 0.0],
        [0.0, -2.0 / 405.0, 0.0, 0.0],
        [0.0, 0.0, 0.0005, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    draw.effect_model_view_projection_matrix = draw.clip_transform;
    let mut payload = vec![0; 128];

    plan.write_payload(&[draw], SceneOwnedUniformFrameInputs::INITIAL, &mut payload)
        .expect("authored-texture retained payload");

    assert_eq!(read_f32(&payload, 0), draw.clip_transform[0][0]);
    assert_eq!(read_f32(&payload, 12), 0.0);
    assert_eq!(read_f32(&payload, 64), draw.clip_transform[0][0]);
    assert_eq!(read_f32(&payload, 76), 0.0);
    assert_eq!(read_f32(&payload, 80), draw.clip_transform[1][0]);
}

#[test]
fn retained_payload_preserves_strided_stereo64_channels() {
    let array = |byte_offset, source| SceneOwnedUniformMemberSource {
        byte_offset,
        byte_size: 1024,
        array_stride: 16,
        source,
    };
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 2048,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 2048,
            members: vec![
                array(
                    0,
                    SceneOwnedRetainedSource::AudioSpectrum {
                        channel: SceneAudioSpectrumChannel::Left,
                        resolution: SceneAudioSpectrumResolution::Bands64,
                    },
                ),
                array(
                    1024,
                    SceneOwnedRetainedSource::AudioSpectrum {
                        channel: SceneAudioSpectrumChannel::Right,
                        resolution: SceneAudioSpectrumResolution::Bands64,
                    },
                ),
            ],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let spectrum = StereoSpectrum64 {
        left: std::array::from_fn(|index| index as f32),
        right: std::array::from_fn(|index| 100.0 + index as f32),
    };
    let mut payload = vec![0xcc; 2048];

    plan.write_payload(
        &[draw()],
        SceneOwnedUniformFrameInputs {
            scalar_overrides: &[],
            scene_time_seconds: 0.0,
            frame_delta_seconds: 0.0,
            audio_spectrum: &spectrum,
            parallax_position: [0.5; 2],
            sampled_binding_phase: 0,
        },
        &mut payload,
    )
    .expect("stereo64 payload");

    assert_eq!(read_f32(&payload, 16), 1.0);
    assert_eq!(read_f32(&payload, 1008), 63.0);
    assert_eq!(read_f32(&payload, 1024), 100.0);
    assert_eq!(read_f32(&payload, 2032), 163.0);
    assert!(payload[4..16].iter().all(|byte| *byte == 0));
    assert!(payload[2036..].iter().all(|byte| *byte == 0));
}

#[test]
fn retained_payload_max_pools_stereo64_into_we_32_and_16_band_arrays() {
    let array = |byte_offset, byte_size, channel, resolution| SceneOwnedUniformMemberSource {
        byte_offset,
        byte_size,
        array_stride: 16,
        source: SceneOwnedRetainedSource::AudioSpectrum {
            channel,
            resolution,
        },
    };
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 1536,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 1536,
            members: vec![
                array(
                    0,
                    512,
                    SceneAudioSpectrumChannel::Left,
                    SceneAudioSpectrumResolution::Bands32,
                ),
                array(
                    512,
                    512,
                    SceneAudioSpectrumChannel::Right,
                    SceneAudioSpectrumResolution::Bands32,
                ),
                array(
                    1024,
                    256,
                    SceneAudioSpectrumChannel::Left,
                    SceneAudioSpectrumResolution::Bands16,
                ),
                array(
                    1280,
                    256,
                    SceneAudioSpectrumChannel::Right,
                    SceneAudioSpectrumResolution::Bands16,
                ),
            ],
        }],
        sampled_slots: Vec::new(),
        phase_resolutions: vec![Vec::new()],
        scene_cover_clip_scale: [1.0, 1.0],
    };
    let spectrum = StereoSpectrum64 {
        left: std::array::from_fn(|index| index as f32),
        right: std::array::from_fn(|index| 100.0 + index as f32),
    };
    let mut payload = vec![0xcc; 1536];

    plan.write_payload(
        &[draw()],
        SceneOwnedUniformFrameInputs {
            scalar_overrides: &[],
            scene_time_seconds: 0.0,
            frame_delta_seconds: 0.0,
            audio_spectrum: &spectrum,
            parallax_position: [0.5; 2],
            sampled_binding_phase: 0,
        },
        &mut payload,
    )
    .expect("downsampled stereo payload");

    assert_eq!(read_f32(&payload, 0), 1.0);
    assert_eq!(read_f32(&payload, 496), 63.0);
    assert_eq!(read_f32(&payload, 512), 101.0);
    assert_eq!(read_f32(&payload, 1008), 163.0);
    assert_eq!(read_f32(&payload, 1024), 3.0);
    assert_eq!(read_f32(&payload, 1264), 63.0);
    assert_eq!(read_f32(&payload, 1280), 103.0);
    assert_eq!(read_f32(&payload, 1520), 163.0);
    assert!(payload[4..16].iter().all(|byte| *byte == 0));
}

fn member(
    byte_offset: u32,
    byte_size: u32,
    source: SceneOwnedRetainedSource,
) -> SceneOwnedUniformMemberSource {
    SceneOwnedUniformMemberSource {
        byte_offset,
        byte_size,
        array_stride: 0,
        source,
    }
}

fn draw() -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        particle_index: crate::engine::scene::INVALID_PARTICLE_INDEX,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        effect_texture_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [1.0; 2],
        uv_inset_texels: 0.0,
        skinning_palette_start: 0,
        skinning_palette_count: 0,
        resolved_color: crate::engine::scene::SceneVec3::default(),
        resolved_alpha: 1.0,
        apply_resolved_visual: false,
        effect_batch_atlas_tile: u32::MAX,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: 0,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: SceneObjectHandle(3),
        material: SceneMaterialHandle(0),
        vertex_start: 0,
        vertex_count: 0,
        index_start: 0,
        index_count: 0,
        instance_count: 1,
    }
}

fn pass_node(
    draw_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> SceneRenderingDevicePassNode {
    SceneRenderingDevicePassNode {
        graph_index: draw_index,
        graph_activation_policy: SceneRenderGraphActivationPolicy::Always,
        pass_record_index: draw_index,
        pass_id: draw_index,
        role: SceneRenderPassKind::EffectMaterial,
        target,
        target_name,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: 0,
        effect_binding_count: 0,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start: draw_index,
        mesh_draw_count: 1,
    }
}

fn read_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}

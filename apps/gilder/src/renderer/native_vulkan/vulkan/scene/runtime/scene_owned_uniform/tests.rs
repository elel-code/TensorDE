use super::*;
use crate::engine::scene::{SceneObjectHandle, SceneRenderingDeviceDrawPrimitive, SceneStringId};

#[test]
fn alignment_uses_the_device_limit_without_power_of_two_assumptions() {
    assert_eq!(align_up(0, 192, "test").unwrap(), 0);
    assert_eq!(align_up(176, 192, "test").unwrap(), 192);
    assert_eq!(align_up(240, 192, "test").unwrap(), 384);
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
    let original = draw.clone();
    let mut payload = vec![0; 192];

    plan.write_payload(
        &[draw.clone()],
        SceneOwnedUniformFrameInputs::INITIAL,
        &mut payload,
    )
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
fn retained_payload_preserves_strided_stereo64_channels() {
    let array = |byte_offset, source| SceneOwnedUniformMemberSource {
        byte_offset,
        byte_size: 1012,
        array_stride: 16,
        source,
    };
    let plan = SceneOwnedUniformArenaPlan {
        byte_count: 2048,
        slices: vec![SceneOwnedUniformSlicePlan {
            draw_index: 0,
            descriptor_lane: 0,
            byte_offset: 0,
            byte_size: 2036,
            members: vec![
                array(0, SceneOwnedRetainedSource::AudioSpectrum64Left),
                array(1024, SceneOwnedRetainedSource::AudioSpectrum64Right),
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
    assert!(payload[2036..].iter().all(|byte| *byte == 0xcc));
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
        shader_key: SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [1.0; 2],
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

fn read_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}

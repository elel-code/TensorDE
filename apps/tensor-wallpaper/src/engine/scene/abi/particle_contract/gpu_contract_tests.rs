use super::*;

#[test]
fn initializer_order_preserves_authored_nibble_sequence() {
    let kinds = [
        SceneParticleInitializerKind::LifetimeRandom,
        SceneParticleInitializerKind::SizeRandom,
        SceneParticleInitializerKind::ColorRandom,
        SceneParticleInitializerKind::TurbulentVelocityRandom,
        SceneParticleInitializerKind::AlphaRandom,
    ];
    let order = SceneParticleInitializerOrder::from_kinds(&kinds).expect("initializer order");
    assert_eq!(order.count(), 5);
    assert_eq!(order.packed_low(), 0x7421);
    assert_eq!(order.packed_high(), 0x0005);
    for (index, expected) in kinds.into_iter().enumerate() {
        assert_eq!(order.kind_at(index), Some(expected));
    }
    assert_eq!(order.kind_at(kinds.len()), None);
    assert_eq!(
        SceneParticleInitializerOrder::from_packed(5, 0x7421, 0x0005),
        Some(order)
    );
    assert!(SceneParticleInitializerOrder::from_packed(5, 0x7421, 0x00f5).is_none());
    assert!(SceneParticleInitializerOrder::from_packed(4, 0x7421, 0x0005).is_none());
}

#[test]
fn gpu_emitter_state_uses_vec4_slots_without_hidden_padding() {
    assert_eq!(std::mem::size_of::<SceneParticleGpuEmitterState>(), 304);
    assert_eq!(std::mem::align_of::<SceneParticleGpuEmitterState>(), 4);
    assert_eq!(std::mem::size_of::<SceneParticleGpuParticleState>(), 64);
}

#[test]
fn indirect_billboard_command_matches_vulkan_layout() {
    assert_eq!(std::mem::size_of::<SceneParticleIndirectDraw>(), 16);
    assert_eq!(
        SceneParticleIndirectDraw::with_instance_count(37).instance_count,
        37
    );
}

#[test]
fn gpu_state_preserves_semantic_particle_values() {
    let mut particle = SceneParticleSystemRecord::unsupported(
        SceneObjectHandle(0),
        SceneResourceId(0),
        SceneMaterialHandle(0),
        9,
        100,
        1.0,
        2.0,
        0.25,
    );
    particle.rate = 12.0;
    particle.gravity = SceneVec3 {
        x: 1.0,
        y: -2.0,
        z: 3.0,
    };
    particle.color_min = SceneVec3 {
        x: 0.88235294,
        y: 0.85813251,
        z: 0.85370512,
    };
    particle.color_max = SceneVec3::ONE;
    let state = SceneParticleGpuEmitterState::from_record(
        &particle,
        80,
        17,
        SceneParticleGpuProfile::RetainedState,
    );
    assert_eq!(
        state.time_scale_rate_start_capacity,
        [0.25, 12.0, 2.0, 80.0]
    );
    assert_eq!(state.gravity, [1.0, -2.0, 3.0, 0.0]);
    assert_eq!(state.emitter_origin[3], 17.0);
    assert_eq!(state.lifetime_min_max_profile_flags[2], 1.0);
    assert_eq!(
        state.color_min_alpha[..3],
        [0.88235294, 0.85813251, 0.85370512]
    );
    assert_eq!(state.color_max_alpha[..3], [1.0, 1.0, 1.0]);

    particle.initializer_order = SceneParticleInitializerOrder::from_kinds(&[
        SceneParticleInitializerKind::LifetimeRandom,
        SceneParticleInitializerKind::SizeRandom,
        SceneParticleInitializerKind::ColorRandom,
        SceneParticleInitializerKind::TurbulentVelocityRandom,
    ])
    .expect("initializer order");
    let state = SceneParticleGpuEmitterState::from_record(
        &particle,
        80,
        17,
        SceneParticleGpuProfile::RetainedState,
    );
    assert_eq!(state.distance_max[3], 4.0);
    assert_eq!(state.velocity_min[3], 0x7421_u16 as f32);
    assert_eq!(state.velocity_max[3], 0.0);
}

//! Binary codec for compiled particle runtime records.

use super::*;

pub(super) fn encode_particles(
    particles: &[SceneParticleSystemRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(particles.len(), "particle count")?);
    for record in particles {
        put_u32(&mut out, record.object.0);
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.parent_particle_index);
        put_u32(&mut out, record.child_type.to_u32());
        put_f32(&mut out, record.child_probability);
        put_u32(&mut out, record.child_max_count);
        put_u32(&mut out, record.simulation.to_u32());
        put_u32(&mut out, record.emitter_shape.to_u32());
        put_u32(&mut out, record.renderer.to_u32());
        put_u32(&mut out, record.module_mask.0);
        put_u32(&mut out, record.initializer_order.count() as u32);
        put_u32(&mut out, record.initializer_order.packed_low() as u32);
        put_u32(&mut out, record.initializer_order.packed_high() as u32);
        put_u32(&mut out, record.flags);
        put_u32(&mut out, record.max_count);
        put_u32(&mut out, record.animation_mode.to_u32());
        put_f32(&mut out, record.sequence_multiplier);
        put_f32(&mut out, record.start_time);
        put_f32(&mut out, record.instance_time_scale);
        put_u32(&mut out, record.instance_color_enabled);
        put_vec3(&mut out, record.instance_color);
        put_vec3(&mut out, record.color_reference);
        put_f32(&mut out, record.instance_count_scale);
        put_f32(&mut out, record.rate);
        put_vec3(&mut out, record.emitter_origin);
        put_vec3(&mut out, record.emitter_directions);
        put_vec3(&mut out, record.distance_min);
        put_vec3(&mut out, record.distance_max);
        put_f32(&mut out, record.emitter_speed_min);
        put_f32(&mut out, record.emitter_speed_max);
        put_f32(&mut out, record.lifetime_min);
        put_f32(&mut out, record.lifetime_max);
        put_f32(&mut out, record.size_min);
        put_f32(&mut out, record.size_max);
        put_vec3(&mut out, record.velocity_min);
        put_vec3(&mut out, record.velocity_max);
        put_vec3(&mut out, record.color_min);
        put_vec3(&mut out, record.color_max);
        put_f32(&mut out, record.alpha_min);
        put_f32(&mut out, record.alpha_max);
        put_f32(&mut out, record.rotation_min);
        put_f32(&mut out, record.rotation_max);
        put_f32(&mut out, record.turbulence_offset);
        put_f32(&mut out, record.turbulence_scale);
        put_f32(&mut out, record.turbulence_speed_min);
        put_f32(&mut out, record.turbulence_speed_max);
        put_f32(&mut out, record.turbulent_velocity_phase_min);
        put_f32(&mut out, record.turbulent_velocity_phase_max);
        put_f32(&mut out, record.turbulent_velocity_time_scale);
        put_vec3(&mut out, record.turbulent_velocity_right);
        put_vec3(&mut out, record.turbulent_velocity_forward);
        put_f32(&mut out, record.turbulence_operator_scale);
        put_f32(&mut out, record.turbulence_operator_speed_min);
        put_f32(&mut out, record.turbulence_operator_speed_max);
        put_f32(&mut out, record.turbulence_operator_phase_min);
        put_f32(&mut out, record.turbulence_phase_max);
        put_f32(&mut out, record.turbulence_operator_time_scale);
        put_vec3(&mut out, record.turbulence_mask);
        put_vec3(&mut out, record.angular_velocity_min);
        put_vec3(&mut out, record.angular_velocity_max);
        put_vec3(&mut out, record.gravity);
        put_f32(&mut out, record.fade_in_time);
        put_f32(&mut out, record.fade_out_time);
        put_f32(&mut out, record.oscillation_frequency_min);
        put_f32(&mut out, record.oscillation_frequency_max);
        put_f32(&mut out, record.oscillation_phase_min);
        put_f32(&mut out, record.oscillation_phase_max);
        put_f32(&mut out, record.oscillation_scale_min);
        put_f32(&mut out, record.oscillation_scale_max);
        put_f32(&mut out, record.position_oscillation_frequency_min);
        put_f32(&mut out, record.position_oscillation_frequency_max);
        put_f32(&mut out, record.position_oscillation_phase_min);
        put_f32(&mut out, record.position_oscillation_phase_max);
        put_f32(&mut out, record.position_oscillation_scale_min);
        put_f32(&mut out, record.position_oscillation_scale_max);
        put_vec3(&mut out, record.position_oscillation_mask);
        put_f32(&mut out, record.size_oscillation_frequency_min);
        put_f32(&mut out, record.size_oscillation_frequency_max);
        put_f32(&mut out, record.size_oscillation_phase_min);
        put_f32(&mut out, record.size_oscillation_phase_max);
        put_f32(&mut out, record.size_oscillation_scale_min);
        put_f32(&mut out, record.size_oscillation_scale_max);
        put_f32(&mut out, record.size_change_start_time);
        put_f32(&mut out, record.size_change_start_value);
        put_f32(&mut out, record.size_change_end_value);
        put_vec3(&mut out, record.vortex_axis);
        put_f32(&mut out, record.vortex_distance_inner);
        put_f32(&mut out, record.vortex_distance_outer);
        put_f32(&mut out, record.vortex_speed_inner);
        put_f32(&mut out, record.vortex_speed_outer);
        put_f32(&mut out, record.trail_length);
        put_f32(&mut out, record.trail_min_length);
        put_f32(&mut out, record.trail_max_length);
        put_u32(&mut out, record.renderer_flags);
    }
    Ok(out)
}

pub(super) fn decode_particles(
    data: &[u8],
) -> Result<Vec<SceneParticleSystemRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut particles = Vec::with_capacity(count);
    for _ in 0..count {
        let object = SceneObjectHandle(decoder.u32()?);
        let resource = decoder.resource_id()?;
        let material = SceneMaterialHandle(decoder.u32()?);
        let parent_particle_index = decoder.u32()?;
        let child_type_raw = decoder.u32()?;
        let child_type = SceneParticleChildType::from_u32(child_type_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("particle child type", child_type_raw),
        )?;
        let child_probability = decoder.f32()?;
        let child_max_count = decoder.u32()?;
        let simulation_raw = decoder.u32()?;
        let simulation = SceneParticleSimulationKind::from_u32(simulation_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("particle simulation kind", simulation_raw),
        )?;
        let emitter_shape_raw = decoder.u32()?;
        let emitter_shape = SceneParticleEmitterShape::from_u32(emitter_shape_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("particle emitter shape", emitter_shape_raw),
        )?;
        let renderer_raw = decoder.u32()?;
        let renderer = SceneParticleRendererKind::from_u32(renderer_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("particle renderer kind", renderer_raw),
        )?;
        let module_mask = SceneParticleModuleMask(decoder.u32()?);
        let initializer_count = decoder.u32()?;
        let initializer_packed_low = decoder.u32()?;
        let initializer_packed_high = decoder.u32()?;
        let initializer_order = SceneParticleInitializerOrder::from_packed(
            initializer_count,
            initializer_packed_low,
            initializer_packed_high,
        )
        .ok_or(SceneBinaryError::InvalidChunkValue(
            "particle initializer order",
            initializer_count,
        ))?;
        particles.push(SceneParticleSystemRecord {
            object,
            resource,
            material,
            parent_particle_index,
            child_type,
            child_probability,
            child_max_count,
            simulation,
            emitter_shape,
            renderer,
            module_mask,
            initializer_order,
            flags: decoder.u32()?,
            max_count: decoder.u32()?,
            animation_mode: {
                let raw = decoder.u32()?;
                SceneParticleAnimationMode::from_u32(raw).ok_or(
                    SceneBinaryError::InvalidChunkValue("particle animation mode", raw),
                )?
            },
            sequence_multiplier: decoder.f32()?,
            start_time: decoder.f32()?,
            instance_time_scale: decoder.f32()?,
            instance_color_enabled: decoder.u32()?,
            instance_color: decoder.vec3()?,
            color_reference: decoder.vec3()?,
            instance_count_scale: decoder.f32()?,
            rate: decoder.f32()?,
            emitter_origin: decoder.vec3()?,
            emitter_directions: decoder.vec3()?,
            distance_min: decoder.vec3()?,
            distance_max: decoder.vec3()?,
            emitter_speed_min: decoder.f32()?,
            emitter_speed_max: decoder.f32()?,
            lifetime_min: decoder.f32()?,
            lifetime_max: decoder.f32()?,
            size_min: decoder.f32()?,
            size_max: decoder.f32()?,
            velocity_min: decoder.vec3()?,
            velocity_max: decoder.vec3()?,
            color_min: decoder.vec3()?,
            color_max: decoder.vec3()?,
            alpha_min: decoder.f32()?,
            alpha_max: decoder.f32()?,
            rotation_min: decoder.f32()?,
            rotation_max: decoder.f32()?,
            turbulence_offset: decoder.f32()?,
            turbulence_scale: decoder.f32()?,
            turbulence_speed_min: decoder.f32()?,
            turbulence_speed_max: decoder.f32()?,
            turbulent_velocity_phase_min: decoder.f32()?,
            turbulent_velocity_phase_max: decoder.f32()?,
            turbulent_velocity_time_scale: decoder.f32()?,
            turbulent_velocity_right: decoder.vec3()?,
            turbulent_velocity_forward: decoder.vec3()?,
            turbulence_operator_scale: decoder.f32()?,
            turbulence_operator_speed_min: decoder.f32()?,
            turbulence_operator_speed_max: decoder.f32()?,
            turbulence_operator_phase_min: decoder.f32()?,
            turbulence_phase_max: decoder.f32()?,
            turbulence_operator_time_scale: decoder.f32()?,
            turbulence_mask: decoder.vec3()?,
            angular_velocity_min: decoder.vec3()?,
            angular_velocity_max: decoder.vec3()?,
            gravity: decoder.vec3()?,
            fade_in_time: decoder.f32()?,
            fade_out_time: decoder.f32()?,
            oscillation_frequency_min: decoder.f32()?,
            oscillation_frequency_max: decoder.f32()?,
            oscillation_phase_min: decoder.f32()?,
            oscillation_phase_max: decoder.f32()?,
            oscillation_scale_min: decoder.f32()?,
            oscillation_scale_max: decoder.f32()?,
            position_oscillation_frequency_min: decoder.f32()?,
            position_oscillation_frequency_max: decoder.f32()?,
            position_oscillation_phase_min: decoder.f32()?,
            position_oscillation_phase_max: decoder.f32()?,
            position_oscillation_scale_min: decoder.f32()?,
            position_oscillation_scale_max: decoder.f32()?,
            position_oscillation_mask: decoder.vec3()?,
            size_oscillation_frequency_min: decoder.f32()?,
            size_oscillation_frequency_max: decoder.f32()?,
            size_oscillation_phase_min: decoder.f32()?,
            size_oscillation_phase_max: decoder.f32()?,
            size_oscillation_scale_min: decoder.f32()?,
            size_oscillation_scale_max: decoder.f32()?,
            size_change_start_time: decoder.f32()?,
            size_change_start_value: decoder.f32()?,
            size_change_end_value: decoder.f32()?,
            vortex_axis: decoder.vec3()?,
            vortex_distance_inner: decoder.f32()?,
            vortex_distance_outer: decoder.f32()?,
            vortex_speed_inner: decoder.f32()?,
            vortex_speed_outer: decoder.f32()?,
            trail_length: decoder.f32()?,
            trail_min_length: decoder.f32()?,
            trail_max_length: decoder.f32()?,
            renderer_flags: decoder.u32()?,
        });
    }
    Ok(particles)
}

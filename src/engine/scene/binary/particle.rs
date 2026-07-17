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
        put_u32(&mut out, record.simulation.to_u32());
        put_u32(&mut out, record.flags);
        put_u32(&mut out, record.max_count);
        put_f32(&mut out, record.sequence_multiplier);
        put_f32(&mut out, record.start_time);
        put_f32(&mut out, record.rate);
        put_vec3(&mut out, record.emitter_origin);
        put_vec3(&mut out, record.emitter_directions);
        put_vec3(&mut out, record.distance_min);
        put_vec3(&mut out, record.distance_max);
        put_f32(&mut out, record.lifetime_min);
        put_f32(&mut out, record.lifetime_max);
        put_f32(&mut out, record.size_min);
        put_f32(&mut out, record.size_max);
        put_vec3(&mut out, record.velocity_min);
        put_vec3(&mut out, record.velocity_max);
        put_vec3(&mut out, record.color_min);
        put_vec3(&mut out, record.color_max);
        put_f32(&mut out, record.rotation_min);
        put_f32(&mut out, record.rotation_max);
        put_f32(&mut out, record.turbulence_offset);
        put_f32(&mut out, record.turbulence_scale);
        put_f32(&mut out, record.turbulence_speed_min);
        put_f32(&mut out, record.turbulence_speed_max);
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
    }
    Ok(out)
}

pub(super) fn decode_particles(
    data: &[u8],
    scene_binary_version: u32,
) -> Result<Vec<SceneParticleSystemRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut particles = Vec::with_capacity(count);
    for _ in 0..count {
        let object = SceneObjectHandle(decoder.u32()?);
        let resource = decoder.resource_id()?;
        let material = SceneMaterialHandle(decoder.u32()?);
        let simulation_raw = decoder.u32()?;
        let simulation = SceneParticleSimulationKind::from_u32(simulation_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("particle simulation kind", simulation_raw),
        )?;
        let mut record = SceneParticleSystemRecord {
            object,
            resource,
            material,
            simulation,
            flags: decoder.u32()?,
            max_count: decoder.u32()?,
            sequence_multiplier: decoder.f32()?,
            start_time: decoder.f32()?,
            rate: decoder.f32()?,
            emitter_origin: decoder.vec3()?,
            emitter_directions: decoder.vec3()?,
            distance_min: decoder.vec3()?,
            distance_max: decoder.vec3()?,
            lifetime_min: decoder.f32()?,
            lifetime_max: decoder.f32()?,
            size_min: decoder.f32()?,
            size_max: decoder.f32()?,
            velocity_min: decoder.vec3()?,
            velocity_max: decoder.vec3()?,
            color_min: decoder.vec3()?,
            color_max: decoder.vec3()?,
            rotation_min: decoder.f32()?,
            rotation_max: decoder.f32()?,
            turbulence_offset: decoder.f32()?,
            turbulence_scale: decoder.f32()?,
            turbulence_speed_min: decoder.f32()?,
            turbulence_speed_max: decoder.f32()?,
            angular_velocity_min: decoder.vec3()?,
            angular_velocity_max: decoder.vec3()?,
            gravity: decoder.vec3()?,
            fade_in_time: decoder.f32()?,
            fade_out_time: decoder.f32()?,
            oscillation_frequency_min: 0.0,
            oscillation_frequency_max: 0.0,
            oscillation_phase_min: 0.0,
            oscillation_phase_max: std::f32::consts::TAU,
            oscillation_scale_min: 1.0,
            oscillation_scale_max: 1.0,
            position_oscillation_frequency_min: 0.0,
            position_oscillation_frequency_max: 0.0,
            position_oscillation_phase_min: 0.0,
            position_oscillation_phase_max: std::f32::consts::TAU,
            position_oscillation_scale_min: 0.0,
            position_oscillation_scale_max: 0.0,
            position_oscillation_mask: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            },
            size_oscillation_frequency_min: 0.0,
            size_oscillation_frequency_max: 0.0,
            size_oscillation_phase_min: 0.0,
            size_oscillation_phase_max: std::f32::consts::TAU,
            size_oscillation_scale_min: 1.0,
            size_oscillation_scale_max: 1.0,
        };
        if scene_binary_version >= 11 {
            record.oscillation_frequency_min = decoder.f32()?;
            record.oscillation_frequency_max = decoder.f32()?;
            if scene_binary_version >= 12 {
                record.oscillation_phase_min = decoder.f32()?;
                record.oscillation_phase_max = decoder.f32()?;
            }
            record.oscillation_scale_min = decoder.f32()?;
            record.oscillation_scale_max = decoder.f32()?;
        }
        if scene_binary_version >= 14 {
            record.position_oscillation_frequency_min = decoder.f32()?;
            record.position_oscillation_frequency_max = decoder.f32()?;
            record.position_oscillation_phase_min = decoder.f32()?;
            record.position_oscillation_phase_max = decoder.f32()?;
            record.position_oscillation_scale_min = decoder.f32()?;
            record.position_oscillation_scale_max = decoder.f32()?;
            record.position_oscillation_mask = decoder.vec3()?;
            record.size_oscillation_frequency_min = decoder.f32()?;
            record.size_oscillation_frequency_max = decoder.f32()?;
            record.size_oscillation_phase_min = decoder.f32()?;
            record.size_oscillation_phase_max = decoder.f32()?;
            record.size_oscillation_scale_min = decoder.f32()?;
            record.size_oscillation_scale_max = decoder.f32()?;
        }
        particles.push(record);
    }
    Ok(particles)
}

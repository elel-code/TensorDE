//! Particle IR lowering into compact runtime-specialized `PART` records.

use crate::engine::scene::*;

use super::{WeIrAmbientSparklesProfile, WeIrFallingLeavesProfile, WeSceneIr};

pub(super) fn lower_particles(ir: &WeSceneIr) -> Vec<SceneParticleSystemRecord> {
    ir.particles
        .iter()
        .map(|particle| {
            let mut record = SceneParticleSystemRecord::unsupported(
                SceneObjectHandle(particle.object),
                SceneResourceId(particle.resource),
                SceneMaterialHandle(particle.material),
                particle.flags,
                particle.max_count,
                particle.sequence_multiplier,
                particle.start_time,
            );
            if let Some(profile) = particle.falling_leaves_profile() {
                apply_falling_leaves_profile(&mut record, profile);
            } else if let Some(profile) = particle.ambient_sparkles_profile() {
                apply_ambient_sparkles_profile(&mut record, profile);
            }
            record
        })
        .collect()
}

fn apply_ambient_sparkles_profile(
    record: &mut SceneParticleSystemRecord,
    profile: WeIrAmbientSparklesProfile,
) {
    record.simulation = SceneParticleSimulationKind::AmbientSparkles;
    record.rate = profile.rate;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = profile.size_min;
    record.size_max = profile.size_max;
    record.color_min = normalized_color(profile.color_min);
    record.color_max = normalized_color(profile.color_max);
    record.gravity = profile.gravity;
    record.fade_in_time = profile.fade_in_time;
    record.fade_out_time = profile.fade_out_time;
    record.oscillation_frequency_min = profile.oscillation_frequency_min;
    record.oscillation_frequency_max = profile.oscillation_frequency_max;
    record.oscillation_phase_min = profile.oscillation_phase_min;
    record.oscillation_phase_max = profile.oscillation_phase_max;
    record.oscillation_scale_min = profile.oscillation_scale_min;
    record.oscillation_scale_max = profile.oscillation_scale_max;
}

fn apply_falling_leaves_profile(
    record: &mut SceneParticleSystemRecord,
    profile: WeIrFallingLeavesProfile,
) {
    record.simulation = SceneParticleSimulationKind::FallingLeaves;
    record.rate = profile.rate;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = profile.size_min;
    record.size_max = profile.size_max;
    record.velocity_min = profile.velocity_min;
    record.velocity_max = profile.velocity_max;
    record.color_min = normalized_color(profile.color_min);
    record.color_max = normalized_color(profile.color_max);
    record.rotation_min = profile.rotation_min;
    record.rotation_max = profile.rotation_max;
    record.turbulence_offset = profile.turbulence_offset;
    record.turbulence_scale = profile.turbulence_scale;
    record.turbulence_speed_min = profile.turbulence_speed_min;
    record.turbulence_speed_max = profile.turbulence_speed_max;
    record.angular_velocity_min = profile.angular_velocity_min;
    record.angular_velocity_max = profile.angular_velocity_max;
    record.gravity = profile.gravity;
    record.fade_in_time = profile.fade_in_time;
    record.fade_out_time = profile.fade_out_time;
}

fn normalized_color(color: SceneVec3) -> SceneVec3 {
    let divisor = if color.x > 1.0 || color.y > 1.0 || color.z > 1.0 {
        255.0
    } else {
        1.0
    };
    SceneVec3 {
        x: color.x / divisor,
        y: color.y / divisor,
        z: color.z / divisor,
    }
}

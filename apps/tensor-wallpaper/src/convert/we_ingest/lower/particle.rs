//! Particle IR lowering into compact runtime-specialized `PART` records.

use crate::engine::scene::*;

use super::{
    WeIrAmbientSparklesProfile, WeIrFallingLeavesProfile, WeIrFloralOscillationProfile,
    WeIrModuleSpriteProfile, WeSceneIr,
};

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
                particle.instance_time_scale,
            );
            record.parent_particle_index = particle.parent_particle_index;
            record.animation_mode = particle.animation_mode;
            record.child_type = match particle.child_type {
                super::WeIrParticleChildType::BuiltinDefault => {
                    SceneParticleChildType::BuiltinDefault
                }
                super::WeIrParticleChildType::Static => SceneParticleChildType::Static,
                super::WeIrParticleChildType::EventFollow => SceneParticleChildType::EventFollow,
                super::WeIrParticleChildType::EventSpawn => SceneParticleChildType::EventSpawn,
                super::WeIrParticleChildType::EventDeath => SceneParticleChildType::EventDeath,
            };
            record.child_probability = particle.child_probability;
            record.child_max_count = particle.child_max_count;
            record.instance_color_enabled = u32::from(particle.instance_color.is_some());
            record.instance_color = particle.instance_color.unwrap_or(SceneVec3::ONE);
            record.color_reference = particle.color_reference;
            record.instance_count_scale = particle.instance_count_scale;
            if let Some(profile) = particle.falling_leaves_profile() {
                apply_falling_leaves_profile(&mut record, profile);
            } else if let Some(profile) = particle.ambient_sparkles_profile() {
                apply_ambient_sparkles_profile(&mut record, profile);
            } else if let Some(profile) = particle.floral_oscillation_profile() {
                apply_floral_oscillation_profile(&mut record, profile);
            } else if let Some(profile) = particle.module_sprite_profile() {
                apply_module_sprite_profile(&mut record, profile);
            }
            (record.color_min, record.color_max) = resolve_particle_color_range(
                record.color_min,
                record.color_max,
                record.color_reference,
                particle.instance_color,
            );
            record
        })
        .collect()
}

fn apply_module_sprite_profile(
    record: &mut SceneParticleSystemRecord,
    profile: WeIrModuleSpriteProfile,
) {
    record.simulation = SceneParticleSimulationKind::ModuleSprite;
    record.emitter_shape = profile.emitter_shape;
    record.renderer = profile.renderer;
    record.module_mask = profile.module_mask;
    record.initializer_order = profile.initializer_order;
    record.renderer_flags = profile.renderer_flags;
    record.rate = profile.rate * record.instance_count_scale;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.emitter_speed_min = profile.emitter_speed_min;
    record.emitter_speed_max = profile.emitter_speed_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = initialized_billboard_size(profile.size_min);
    record.size_max = initialized_billboard_size(profile.size_max);
    record.velocity_min = profile.velocity_min;
    record.velocity_max = profile.velocity_max;
    record.color_min = normalized_color(profile.color_min);
    record.color_max = normalized_color(profile.color_max);
    record.alpha_min = profile.alpha_min;
    record.alpha_max = profile.alpha_max;
    record.rotation_min = profile.rotation_min;
    record.rotation_max = profile.rotation_max;
    record.turbulence_offset = profile.turbulence_offset;
    record.turbulence_scale = profile.turbulence_scale;
    record.turbulence_speed_min = profile.turbulence_speed_min;
    record.turbulence_speed_max = profile.turbulence_speed_max;
    record.turbulent_velocity_phase_min = profile.turbulent_velocity_phase_min;
    record.turbulent_velocity_phase_max = profile.turbulent_velocity_phase_max;
    record.turbulent_velocity_time_scale = profile.turbulent_velocity_time_scale;
    record.turbulent_velocity_right = profile.turbulent_velocity_right;
    record.turbulent_velocity_forward = profile.turbulent_velocity_forward;
    record.turbulence_operator_scale = profile.turbulence_operator_scale;
    record.turbulence_operator_speed_min = profile.turbulence_operator_speed_min;
    record.turbulence_operator_speed_max = profile.turbulence_operator_speed_max;
    record.turbulence_operator_phase_min = profile.turbulence_operator_phase_min;
    record.turbulence_phase_max = profile.turbulence_phase_max;
    record.turbulence_operator_time_scale = profile.turbulence_operator_time_scale;
    record.turbulence_mask = profile.turbulence_mask;
    record.gravity = profile.gravity;
    record.fade_in_time = profile.fade_in_time;
    record.fade_out_time = profile.fade_out_time;
    record.position_oscillation_frequency_min = profile.position_frequency_min;
    record.position_oscillation_frequency_max = profile.position_frequency_max;
    record.position_oscillation_phase_min = profile.position_phase_min;
    record.position_oscillation_phase_max = profile.position_phase_max;
    record.position_oscillation_scale_min = profile.position_scale_min;
    record.position_oscillation_scale_max = profile.position_scale_max;
    record.position_oscillation_mask = profile.position_mask;
    record.size_change_start_time = profile.size_change_start_time;
    record.size_change_start_value = profile.size_change_start_value;
    record.size_change_end_value = profile.size_change_end_value;
    record.vortex_axis = profile.vortex_axis;
    record.vortex_distance_inner = profile.vortex_distance_inner;
    record.vortex_distance_outer = profile.vortex_distance_outer;
    record.vortex_speed_inner = profile.vortex_speed_inner;
    record.vortex_speed_outer = profile.vortex_speed_outer;
    record.trail_length = profile.trail_length;
    record.trail_min_length = profile.trail_min_length;
    record.trail_max_length = profile.trail_max_length;
}

fn apply_floral_oscillation_profile(
    record: &mut SceneParticleSystemRecord,
    profile: WeIrFloralOscillationProfile,
) {
    record.simulation = SceneParticleSimulationKind::FloralOscillation;
    record.renderer = SceneParticleRendererKind::Sprite;
    record.rate = profile.rate * record.instance_count_scale;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = initialized_billboard_size(profile.size_min);
    record.size_max = initialized_billboard_size(profile.size_max);
    record.rotation_min = profile.rotation_min;
    record.rotation_max = profile.rotation_max;
    record.position_oscillation_frequency_min = profile.position_frequency_min;
    record.position_oscillation_frequency_max = profile.position_frequency_max;
    record.position_oscillation_phase_min = profile.position_phase_min;
    record.position_oscillation_phase_max = profile.position_phase_max;
    record.position_oscillation_scale_min = profile.position_scale_min;
    record.position_oscillation_scale_max = profile.position_scale_max;
    record.position_oscillation_mask = profile.position_mask;
    record.size_oscillation_frequency_min = profile.size_frequency_min;
    record.size_oscillation_frequency_max = profile.size_frequency_max;
    record.size_oscillation_phase_min = profile.size_phase_min;
    record.size_oscillation_phase_max = profile.size_phase_max;
    record.size_oscillation_scale_min = profile.size_scale_min;
    record.size_oscillation_scale_max = profile.size_scale_max;
}

fn apply_ambient_sparkles_profile(
    record: &mut SceneParticleSystemRecord,
    profile: WeIrAmbientSparklesProfile,
) {
    record.simulation = SceneParticleSimulationKind::AmbientSparkles;
    record.renderer = SceneParticleRendererKind::Sprite;
    record.rate = profile.rate * record.instance_count_scale;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = initialized_billboard_size(profile.size_min);
    record.size_max = initialized_billboard_size(profile.size_max);
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
    record.renderer = SceneParticleRendererKind::Sprite;
    record.rate = profile.rate * record.instance_count_scale;
    record.emitter_origin = profile.emitter_origin;
    record.emitter_directions = profile.emitter_directions;
    record.distance_min = profile.distance_min;
    record.distance_max = profile.distance_max;
    record.lifetime_min = profile.lifetime_min;
    record.lifetime_max = profile.lifetime_max;
    record.size_min = initialized_billboard_size(profile.size_min);
    record.size_max = initialized_billboard_size(profile.size_max);
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

fn initialized_billboard_size(authored_size: f32) -> f32 {
    // WE initializes the per-particle size multiplier to 0.5 before sizerandom runs.
    // The genericparticle shader then expands that initialized span around uv 0.5.
    authored_size * 0.5
}

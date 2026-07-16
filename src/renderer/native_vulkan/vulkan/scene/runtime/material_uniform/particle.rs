//! Per-emitter GPU particle simulation constants.

use super::*;

pub(super) fn particle_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let Some(particle) = storage.particle_for_object(draw.object) else {
        return values;
    };
    values[0] = draw.resolved_color.x;
    values[1] = draw.resolved_color.y;
    values[2] = draw.resolved_color.z;
    values[3] = draw.resolved_alpha;
    values[4] = scene_time_seconds;
    values[5] = particle.start_time;
    values[6] = particle.rate;
    values[7] = particle.max_count as f32;
    values[8] = particle.sequence_multiplier;
    values[9] = particle.lifetime_min;
    values[10] = particle.lifetime_max;
    values[11] = particle.size_min;
    values[12] = particle.size_max;
    values[13] = particle.fade_in_time;
    values[14] = particle.fade_out_time;
    values[15] = draw.object.0 as f32 * 37.0 + 11.0;
    write_vec3(&mut values, 16, particle.emitter_origin);
    write_vec3(&mut values, 20, particle.emitter_directions);
    write_vec3(&mut values, 24, particle.distance_min);
    write_vec3(&mut values, 28, particle.distance_max);
    write_vec3(&mut values, 32, particle.velocity_min);
    write_vec3(&mut values, 36, particle.velocity_max);
    write_vec3(&mut values, 40, particle.gravity);
    values[44] = particle.turbulence_offset;
    values[45] = particle.turbulence_scale;
    values[46] = particle.turbulence_speed_min;
    values[47] = particle.turbulence_speed_max;
    write_vec3(&mut values, 48, particle.angular_velocity_min);
    write_vec3(&mut values, 52, particle.angular_velocity_max);
    values[56] = particle.rotation_min;
    values[57] = particle.rotation_max;
    write_vec3(&mut values, 60, particle.color_min);
    write_vec3(&mut values, 64, particle.color_max);
    values
}

fn write_vec3(values: &mut [f32], start: usize, value: crate::engine::scene::SceneVec3) {
    values[start] = value.x;
    values[start + 1] = value.y;
    values[start + 2] = value.z;
}

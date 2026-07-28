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
    values[4] = scene_time_seconds * particle.instance_time_scale;
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
    values[68] = particle.simulation.to_u32() as f32;
    values[69] = particle.oscillation_frequency_min;
    values[70] = particle.oscillation_frequency_max;
    values[71] = particle.oscillation_scale_min;
    values[72] = particle.oscillation_scale_max;
    values[73] = particle_texture_decode_mode(storage, draw);
    values[74] = particle.oscillation_phase_min;
    values[75] = particle.oscillation_phase_max;
    values[76..80].copy_from_slice(&particle_billboard_inverse(storage, draw));
    values[80] = particle_texture_aspect(storage, draw);
    values[84] = particle.position_oscillation_frequency_min;
    values[85] = particle.position_oscillation_frequency_max;
    values[86] = particle.position_oscillation_phase_min;
    values[87] = particle.position_oscillation_phase_max;
    values[88] = particle.position_oscillation_scale_min;
    values[89] = particle.position_oscillation_scale_max;
    values[90] = particle.position_oscillation_mask.x;
    values[91] = particle.position_oscillation_mask.y;
    values[92] = particle.size_oscillation_frequency_min;
    values[93] = particle.size_oscillation_frequency_max;
    values[94] = particle.size_oscillation_phase_min;
    values[95] = particle.size_oscillation_phase_max;
    values[96] = particle.size_oscillation_scale_min;
    values[97] = particle.size_oscillation_scale_max;
    values[98] = particle.position_oscillation_mask.z;
    values
}

fn particle_texture_decode_mode(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> f32 {
    particle_texture(storage, draw)
        .is_some_and(|texture| texture.source_runtime_format == 8)
        .then_some(1.0)
        .unwrap_or(0.0)
}

fn particle_texture<'storage>(
    storage: &'storage SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> Option<&'storage crate::engine::scene::SceneTextureRecord> {
    let material = storage.material(draw.material)?;
    storage
        .material_passes(material)
        .iter()
        .flat_map(|pass| storage.material_pass_textures(pass))
        .find(|binding| binding.slot == 0)
        .and_then(|binding| storage.texture(binding.resource))
}

fn particle_texture_aspect(storage: &SceneStorage, draw: &SceneRenderingDeviceMeshDraw) -> f32 {
    particle_texture(storage, draw).map_or(1.0, |texture| {
        texture.height.max(1) as f32 / texture.width.max(1) as f32
    })
}

fn particle_billboard_inverse(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; 4] {
    let width = storage.project().logical_width.max(1) as f32;
    let height = storage.project().logical_height.max(1) as f32;
    let linear = [
        draw.clip_transform[0][0] * width * 0.5,
        draw.clip_transform[0][1] * width * 0.5,
        -draw.clip_transform[1][0] * height * 0.5,
        -draw.clip_transform[1][1] * height * 0.5,
    ];
    inverse_linear_2d(linear).unwrap_or([1.0, 0.0, 0.0, 1.0])
}

fn inverse_linear_2d([m00, m01, m10, m11]: [f32; 4]) -> Option<[f32; 4]> {
    let determinant = m00 * m11 - m01 * m10;
    if !determinant.is_finite() || determinant.abs() <= 1.0e-6 {
        return None;
    }
    let inverse = 1.0 / determinant;
    Some([m11 * inverse, -m01 * inverse, -m10 * inverse, m00 * inverse])
}

#[cfg(test)]
mod tests {
    use super::inverse_linear_2d;

    #[test]
    fn billboard_inverse_cancels_object_rotation_and_scale() {
        let inverse = inverse_linear_2d([0.0, -3.0, 2.0, 0.0]).expect("invertible");
        assert_eq!(inverse, [0.0, 0.5, -1.0 / 3.0, 0.0]);
    }
}

fn write_vec3(values: &mut [f32], start: usize, value: crate::engine::scene::SceneVec3) {
    values[start] = value.x;
    values[start + 1] = value.y;
    values[start + 2] = value.z;
}

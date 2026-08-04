//! Per-emitter GPU particle simulation constants.

use super::*;

pub(super) fn particle_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let Some(particle) = storage.particle(draw.particle_index) else {
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
    values[15] = draw.particle_index as f32 * 37.0 + 11.0;
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
    values[76..80].copy_from_slice(&particle_billboard_orientation());
    values[80] = particle_texture_aspect(storage, draw);
    values[81] = particle.alpha_min;
    values[82] = particle.alpha_max;
    values[83] = particle.emitter_shape.to_u32() as f32;
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
    values[99] = particle.module_mask.0 as f32;
    write_vec3(&mut values, 100, particle.turbulence_mask);
    values[103] = particle.turbulence_operator_scale;
    values[104] = particle.turbulence_phase_max;
    values[105] = particle.turbulent_velocity_phase_min;
    values[106] = particle.turbulent_velocity_phase_max;
    values[107] = particle.turbulent_velocity_time_scale;
    values[108] = particle.size_change_start_time;
    values[109] = particle.size_change_start_value;
    values[110] = particle.size_change_end_value;
    values[111] = particle.renderer.to_u32() as f32;
    write_vec3(&mut values, 112, particle.vortex_axis);
    values[115] = particle.vortex_distance_inner;
    values[116] = particle.vortex_distance_outer;
    values[117] = particle.vortex_speed_inner;
    values[118] = particle.vortex_speed_outer;
    values[119] = particle.trail_length;
    values[120] = particle.trail_min_length;
    values[121] = particle.renderer_flags as f32;
    write_vec3(&mut values, 124, particle.turbulent_velocity_right);
    values[127] = particle.turbulence_operator_speed_min;
    write_vec3(&mut values, 128, particle.turbulent_velocity_forward);
    values[131] = particle.turbulence_operator_speed_max;
    values[132] = particle.turbulence_operator_phase_min;
    values[133] = particle.turbulence_operator_time_scale;
    values[134] = particle.gpu_profile() as u32 as f32;
    values[135] = storage
        .particles()
        .iter()
        .take(draw.particle_index as usize)
        .filter(|candidate| candidate.max_count != 0)
        .map(|candidate| candidate.procedural_instance_capacity())
        .fold(0u32, u32::saturating_add) as f32;
    values[136..140].copy_from_slice(&particle_trail_eye_and_max(storage, draw));
    values[140..148].copy_from_slice(&particle_texture_sequence(storage, draw));
    values
}

fn particle_texture_sequence(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; 8] {
    let Some(texture) = particle_texture(storage, draw) else {
        return [0.0; 8];
    };
    let Some(first) = storage.texture_sequence_frames(texture).first() else {
        return [0.0; 8];
    };
    let width = first.axis_x[0];
    let height = first.axis_y[1];
    [
        width,
        height,
        texture.sequence_frame_count as f32,
        height / width,
        first.origin[0],
        first.origin[1],
        0.0,
        0.0,
    ]
}

fn particle_trail_eye_and_max(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; 4] {
    let Some(particle) = storage.particle(draw.particle_index) else {
        return [0.0; 4];
    };
    let width = storage.project().logical_width.max(1) as f32;
    let height = storage.project().logical_height.max(1) as f32;
    let world_x = draw.render_world_matrix[0];
    let world_y = draw.render_world_matrix[1];
    let world_x_length_squared = world_x[..3].iter().map(|value| value * value).sum::<f32>();
    let clip_world_x_dot = draw.clip_transform[0][..3]
        .iter()
        .zip(world_x[..3].iter())
        .map(|(clip, world)| clip * world)
        .sum::<f32>();
    let zoom = if world_x_length_squared > 1.0e-8 {
        clip_world_x_dot / world_x_length_squared * width * 0.5
    } else {
        1.0
    };
    let zoom = if zoom.is_finite() && zoom.abs() > 1.0e-6 {
        zoom
    } else {
        1.0
    };
    let camera_x = world_x[3] - draw.clip_transform[0][3] * width / (2.0 * zoom);
    let camera_y = world_y[3] + draw.clip_transform[1][3] * height / (2.0 * zoom);
    let eye_world = [
        width * 0.5 + camera_x,
        height * 0.5 + camera_y,
        2_000.0,
        1.0,
    ];
    let inverse = super::super::shader_uniform::inverse_affine_rows(&draw.render_world_matrix)
        .unwrap_or([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
    let local_eye: [f32; 3] = std::array::from_fn(|row_index| {
        inverse[row_index]
            .iter()
            .zip(eye_world)
            .map(|(matrix, eye)| matrix * eye)
            .sum()
    });
    [
        local_eye[0],
        local_eye[1],
        local_eye[2],
        particle.trail_max_length,
    ]
}

fn particle_texture_decode_mode(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> f32 {
    if particle_texture(storage, draw).is_some_and(|texture| texture.source_runtime_format == 8) {
        1.0
    } else {
        0.0
    }
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

fn particle_billboard_orientation() -> [f32; 4] {
    // WE expands particle corners in the emitter's local coordinate system and applies the
    // complete authored object transform afterward. Its 2D scene camera contributes unit
    // orientation axes; cancelling the object's scale or rotation here changes authored
    // particle geometry.
    [1.0, 0.0, 0.0, 1.0]
}

fn write_vec3(values: &mut [f32], start: usize, value: crate::engine::scene::SceneVec3) {
    values[start] = value.x;
    values[start + 1] = value.y;
    values[start + 2] = value.z;
}

#[cfg(test)]
mod tests {
    use super::particle_billboard_orientation;

    #[test]
    fn billboard_orientation_preserves_authored_local_axes() {
        assert_eq!(particle_billboard_orientation(), [1.0, 0.0, 0.0, 1.0]);
    }
}

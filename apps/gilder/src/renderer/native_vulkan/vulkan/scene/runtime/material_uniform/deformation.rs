use super::*;

pub(super) fn shake_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speed"], 1.0);
    values[2] = parameters.scalar(&["strength"], 0.1);
    values[4..6].copy_from_slice(&[0.0, 1.0]);
    values[6..8].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["bounds"]), 2);
    set_vector(&mut values, 6, &parameters.values(&["friction"]), 2);
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn foliage_sway_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speeduv", "speed"], 5.0);
    values[2] = parameters.scalar(&["strength"], 0.4);
    values[3] = parameters.scalar(&["phase"], 0.5);
    values[4] = parameters.scalar(&["power"], 1.0);
    values[5] = parameters.scalar(&["scale", "noisescale"], 0.05);
    values[6] = parameters.scalar(&["ratio"], 0.3);
    values[7] = parameters.scalar(&["scrolldirection", "direction"], 0.0);
    values[8..12].copy_from_slice(&object_source_texture_resolution(storage, draw.object));
    values
}

pub(super) fn foliage_ripple_composite_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&[1.0; 4]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["base.color4", "base.g_color4", "base.color", "base.tint"]),
        4,
    );
    values[0] *= draw.resolved_color.x;
    values[1] *= draw.resolved_color.y;
    values[2] *= draw.resolved_color.z;
    values[3] *= draw.resolved_alpha;
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["foliage.speeduv", "foliage.speed"], 5.0);
    values[6] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["foliage.strength"], 0.4)
    } else {
        0.0
    };
    values[7] = parameters.scalar(&["foliage.phase"], 0.5);
    values[8] = parameters.scalar(&["foliage.power"], 1.0);
    values[9] = parameters.scalar(&["foliage.scale", "foliage.noisescale"], 0.05);
    values[10] = parameters.scalar(&["foliage.ratio"], 0.3);
    values[11] = parameters.scalar(&["foliage.scrolldirection", "foliage.direction"], 0.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values[16] = scene_time_seconds;
    values[17] = parameters.scalar(&["ripple.animationspeed"], 0.15);
    values[18] = parameters.scalar(&["ripple.scale"], 1.0);
    values[19] = parameters.scalar(&["ripple.scrollspeed"], 0.0);
    values[20] = parameters.scalar(&["ripple.scrolldirection", "ripple.direction"], 0.0);
    values[21] = if draw_effect_enabled(draw, 1) {
        parameters.scalar(&["ripple.ripplestrength", "ripple.strength"], 0.1)
    } else {
        0.0
    };
    let ripple_ratio = parameters.scalar(&["ripple.ratio"], 1.0);
    values[22] = ripple_ratio;
    values[23] = ripple_ratio;
    values
}

pub(super) fn waterwaves_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let speed = parameters.scalar(&["speed"], 5.0);
    let scale = parameters.scalar(&["scale"], 200.0);
    values[0] = scene_time_seconds;
    values[1] = speed;
    values[2] = scale;
    values[3] = parameters.scalar(&["strength"], 0.1);
    values[4] = parameters.scalar(&["direction"], 0.0);
    values[5] = parameters.scalar(&["speed2"], speed);
    values[6] = parameters.scalar(&["scale2"], scale);
    values[7] = parameters.scalar(&["direction2"], 0.0);
    values[8] = parameters.scalar(&["offset2"], 0.0);
    values[9] = bool_float(shader_combo_enabled(shader_key, "DUALWAVES"));
    values[10] = parameters.scalar(&["exponent"], 1.0);
    values[11] = parameters.scalar(&["exponent2"], 1.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn waterwaves_uv_field_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    waterwaves_displacement_values(parameters, storage, draw, scene_time_seconds, 0, 4)
}

pub(super) fn waterwaves_direct_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values =
        waterwaves_displacement_values(parameters, storage, draw, scene_time_seconds, 4, 8);
    let (resolved_color, resolved_alpha) = if draw.apply_resolved_visual {
        (draw.resolved_color, draw.resolved_alpha)
    } else {
        (
            crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            1.0,
        )
    };
    let standard = standard_material_values(parameters, resolved_color, resolved_alpha);
    values[..4].copy_from_slice(&standard[..4]);
    values
}

fn waterwaves_displacement_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    chain_base: usize,
    stage_start: usize,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    const MAX_STAGES: usize = 9;
    const STAGE_FLOATS: usize = 16;
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[chain_base] = parameters
        .scalar(&["waterwaves.stage_count"], 0.0)
        .clamp(0.0, MAX_STAGES as f32);
    values[chain_base + 1] = scene_time_seconds;
    for stage in 0..MAX_STAGES {
        let speed = waterwaves_stage_scalar(parameters, stage, "speed", 5.0);
        let scale = waterwaves_stage_scalar(parameters, stage, "scale", 200.0);
        let strength = waterwaves_stage_scalar(parameters, stage, "strength", 0.1);
        let direction_angle = waterwaves_stage_scalar(parameters, stage, "direction", 0.0);
        let speed2 = waterwaves_stage_scalar(parameters, stage, "speed2", speed);
        let scale2 = waterwaves_stage_scalar(parameters, stage, "scale2", scale);
        let direction2_angle = waterwaves_stage_scalar(parameters, stage, "direction2", 0.0);
        let offset2 = waterwaves_stage_scalar(parameters, stage, "offset2", 0.0);
        let dual_waves = waterwaves_stage_scalar(parameters, stage, "dualwaves", 0.0) > 0.5;
        let direction = [-direction_angle.sin(), direction_angle.cos()];
        let direction2 = [-direction2_angle.sin(), direction2_angle.cos()];
        let base = stage_start + stage * STAGE_FLOATS;
        values[base] = scene_time_seconds * speed;
        values[base + 1] = scale;
        values[base + 2] = if draw_effect_enabled(draw, stage) {
            strength * strength
        } else {
            0.0
        };
        values[base + 3] = waterwaves_stage_scalar(parameters, stage, "mask", 0.0);
        values[base + 4..base + 6].copy_from_slice(&direction);
        values[base + 6] = (scene_time_seconds + offset2) * speed2;
        values[base + 7] = if dual_waves { scale2 } else { 0.0 };
        values[base + 8..base + 10].copy_from_slice(&direction2);
        values[base + 10] = waterwaves_stage_scalar(parameters, stage, "exponent", 1.0);
        values[base + 11] = waterwaves_stage_scalar(parameters, stage, "exponent2", 1.0);
        values[base + 12..base + 16].copy_from_slice(&material_texture_resolution(
            storage,
            parameters.pass,
            stage as u32 + 1,
        ));
    }
    values
}

pub(super) fn draw_effect_enabled(draw: &SceneRenderingDeviceMeshDraw, local_index: usize) -> bool {
    draw.effect_visibility_policy == crate::engine::scene::SceneRenderEffectVisibilityPolicy::None
        || (local_index < draw.effect_binding_count as usize
            && local_index < 32
            && draw.resolved_effect_visibility_mask & (1 << local_index) != 0)
}

fn waterwaves_stage_scalar(
    parameters: &MaterialParameters<'_>,
    stage: usize,
    name: &str,
    default: f32,
) -> f32 {
    let name = format!("waterwaves.{stage}.{name}");
    parameters.scalar(&[name.as_str()], default)
}

pub(super) fn waterripple_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let ratio = parameters.scalar(&["ratio"], 1.0);
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["animationspeed"], 0.15);
    values[2] = parameters.scalar(&["scale"], 1.0);
    values[3] = parameters.scalar(&["scrollspeed"], 0.0);
    values[4] = parameters.scalar(&["scrolldirection", "direction"], 0.0);
    values[5] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["ripplestrength", "strength"], 0.1)
    } else {
        0.0
    };
    values[6] = ratio;
    values[7] = 1.0;
    values[8] = bool_float(shader_texture_slot_enabled(shader_key, 1));
    values[9] = bool_float(shader_texture_slot_enabled(shader_key, 2));
    values[11] = ratio;
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

use super::*;

pub(super) fn blend_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["multiply"], 1.0);
    values[1] = parameters.scalar(&["alpha"], 1.0);
    values[2] = parameters.scalar(&["blendangle"], 0.0);
    values[3] = parameters.scalar(&["blendscale"], 1.0);
    set_vector(&mut values, 4, &parameters.values(&["blendoffset"]), 2);
    values[6] = bool_float(shader_combo_enabled(shader_key, "TRANSFORMUV"));
    values[7] = shader_combo_value(shader_key, "TRANSFORMREPEAT", 0) as f32;
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn blend_gradient_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = blend_values(parameters, storage, shader_key);
    values[16] = parameters.scalar(&["gradientscale"], 0.05);
    values[17] = parameters.scalar(&["edgebrightness"], 1.0);
    values[20..24].copy_from_slice(&[1.0, 0.75, 0.0, 1.0]);
    set_vector(&mut values, 20, &parameters.values(&["edgecolor"]), 3);
    values
}

pub(super) fn shimmer_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["ui_editor_properties_direction", "direction"], 1.5707964);
    values[2] = parameters.scalar(&["ui_editor_properties_granularity", "scale"], 1.0);
    values[3] = parameters.scalar(&["ui_editor_properties_speed", "speed"], 1.0);
    values[4] = parameters.scalar(&["ui_editor_properties_delay", "delay"], 2.0);
    values[5] = parameters.scalar(&["ui_editor_properties_width", "width"], 1.0);
    values[6] = parameters.scalar(&["ui_editor_properties_brightness", "amount"], 1.0);
    values[7] = parameters.scalar(&["ui_editor_properties_offset", "offset"], 0.0);
    values[8] = parameters.scalar(&["ui_editor_properties_timescale", "timeoffsetscale"], 0.05);
    // The Slang shader packs `u_timeoffsetScale` followed by `u_color.rgb` in one vec4.
    // Keep this aligned with ShimmerUniform::g_TimeOffsetColor in the generated shader.
    values[9..12].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        9,
        &parameters.values(&["ui_editor_properties_color", "color"]),
        3,
    );
    values
}

pub(super) fn tint_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["alpha"], 1.0);
    values[1..4].copy_from_slice(&[1.0, 0.0, 0.0]);
    set_vector(&mut values, 1, &parameters.values(&["color"]), 3);
    values[4..8].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

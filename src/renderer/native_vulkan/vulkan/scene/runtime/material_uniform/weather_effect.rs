use super::*;

pub(super) fn lightning_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speed"], 0.3);
    values[2] = parameters.scalar(&["erratic"], 1.0);
    values[3] = parameters.scalar(&["amount"], 1.0);
    values[4] = parameters.scalar(&["power"], 1.0);
    values[5] = parameters.scalar(&["brightness"], 1.0);
    values[8..12].copy_from_slice(&[0.7, 0.8, 1.0, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["color"]), 3);
    values
}

pub(super) fn swing_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["amount"], 0.2);
    values[2] = parameters.scalar(&["speed"], 2.0);
    values[3] = parameters.scalar(&["phase"], 0.0);
    values[4] = parameters.scalar(&["size"], 0.4);
    values[5] = parameters.scalar(&["center"], 0.5);
    values[6] = parameters.scalar(&["feather"], 0.01);
    values[8..10].copy_from_slice(&[0.25, 0.5]);
    set_vector(&mut values, 8, &parameters.values(&["point0"]), 2);
    values[10..12].copy_from_slice(&[0.75, 0.5]);
    set_vector(&mut values, 10, &parameters.values(&["point1"]), 2);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values
}

pub(super) fn raindrop_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["Rain Amount"], 0.7);
    values[2] = parameters.scalar(&["Background Blur"], 1.0);
    values[3] = parameters.scalar(&["Rain Speed"], 0.2);
    values[4] = parameters.scalar(&["Drop Density"], 6.0);
    values[5] = parameters.scalar(&["Fog Strength"], 0.18);
    values[6] = parameters.scalar(&["Vignette Strength"], 0.9);
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values[12..16].copy_from_slice(&[0.1, 0.11, 0.12, 1.0]);
    set_vector(&mut values, 12, &parameters.values(&["Drop Shadow Color"]), 3);
    values[16..20].copy_from_slice(&[0.04, 0.05, 0.06, 1.0]);
    set_vector(&mut values, 16, &parameters.values(&["Drop Highlight Color"]), 3);
    values
}

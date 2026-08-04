use super::*;

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

//! Typed material packing for Wallpaper Engine's installed Depth Parallax effect.

use super::*;

pub(super) fn depth_parallax_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    parallax_position: [f32; 2],
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values[4..8].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 2));
    values[8..10].copy_from_slice(&parallax_position);
    values[10..12].copy_from_slice(&[1.0; 2]);
    set_vector(&mut values, 10, &parameters.values(&["scale"]), 2);
    values[12] = parameters.scalar(&["sens"], 1.0);
    values[13] = parameters.scalar(&["center"], 0.3);
    values
}

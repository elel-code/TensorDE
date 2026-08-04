//! Typed material packing for Wallpaper Engine's installed Pulse effect.

use super::*;
use crate::engine::scene::StereoSpectrum64;

pub(super) fn pulse_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
    scene_time_seconds: f32,
    audio_spectrum: Option<&StereoSpectrum64>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = bool_float(shader_combo_value(shader_key, "PULSEALPHA", 0) != 0);
    values[1] = bool_float(shader_combo_value(shader_key, "PULSECOLOR", 1) != 0);

    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["speed"], 3.0);
    values[6] = parameters.scalar(&["phase"], 0.0);
    values[7] = parameters.scalar(&["amount"], 1.0);

    values[8..10].copy_from_slice(&[0.0, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["bounds"]), 2);
    values[10] = parameters.scalar(&["power"], 1.0);
    values[11] = parameters.scalar(&["noisespeed"], 0.5);

    values[12] = parameters.scalar(&["noiseamount"], 0.0);
    values[13] = parameters.scalar(&["frequencymin"], 0.0);
    values[14] = parameters.scalar(&["frequencymax"], 1.0);
    values[15] = parameters.scalar(&["audioexponent"], 1.0);

    values[16..18].copy_from_slice(&[0.5, 1.0]);
    set_vector(&mut values, 16, &parameters.values(&["audiobounds"]), 2);
    values[18] = parameters.scalar(&["audioamount"], 1.0);

    values[20..23].copy_from_slice(&[1.0; 3]);
    set_vector(&mut values, 20, &parameters.values(&["tintlow"]), 3);
    values[24..27].copy_from_slice(&[1.0; 3]);
    set_vector(&mut values, 24, &parameters.values(&["tinthigh"]), 3);
    values[28..32].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 2));

    let spectrum = audio_spectrum.copied().unwrap_or(StereoSpectrum64::ZERO);
    let left32 = StereoSpectrum64::max_pool_32(&spectrum.left);
    let right32 = StereoSpectrum64::max_pool_32(&spectrum.right);
    values[32..48].copy_from_slice(&StereoSpectrum64::max_pool_16(&left32));
    values[48..64].copy_from_slice(&StereoSpectrum64::max_pool_16(&right32));
    values
}

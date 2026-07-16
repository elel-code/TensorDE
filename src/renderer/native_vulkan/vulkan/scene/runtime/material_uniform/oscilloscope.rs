use super::*;

pub(super) fn oscilloscope_values(
    parameters: &MaterialParameters<'_>,
    spectrum: Option<&[f32; 32]>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    set_vector(&mut values, 0, &parameters.values(&["Color"]), 3);
    values[3] = parameters.scalar(&["Opacity"], 1.0);
    values[4] = parameters.scalar(&["Brightness"], 1.0);
    values[5] = parameters.scalar(&["Amplitude"], 1.0);
    values[6] = parameters.scalar(&["Height"], 0.5);
    values[7] = parameters.scalar(&["Thickness"], 0.5);
    values[8] = parameters.scalar(&["Smoothness"], 0.5);
    values[9] = parameters.scalar(&["Frequency exponent"], 5.5);
    values[10] = parameters.scalar(&["Scope"], 2.0);
    values[11] = parameters.scalar(&["Flow speed"], 1.0);
    values[12] = parameters.scalar(&["Offset"], 0.0);
    values[13] = parameters.scalar(&["angle"], 0.0);
    values[14] = parameters.scalar(&["Amplitude exponent"], 1.0);
    if let Some(spectrum) = spectrum {
        values[16..32].copy_from_slice(&spectrum[..16]);
    }
    values
}
